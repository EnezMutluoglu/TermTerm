// Callback checks use local input frames only; no VPN, network or privileged calls.
#define main termterm_openconnect_main
#include "../vpn-engine/openconnect.cpp"
#undef main
#include <cassert>
#include <climits>
#include <cstdio>
#include <sstream>

static void input(const std::string &bytes) {
    auto *file = std::tmpfile();
    assert(file);
    assert(std::fwrite(bytes.data(), 1, bytes.size(), file) == bytes.size());
    std::rewind(file);
    assert(dup2(fileno(file), STDIN_FILENO) >= 0);
    std::fclose(file);
}
static void answer(Json::Value values, unsigned id = 0) {
    Json::Value message;
    message["action"] = "answer";
    message["id"] = id ? id : challenge + 1;
    message["values"] = std::move(values);
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    input(Json::writeString(writer, message) + "\n");
}
static void secret_free(const AuthDiagnostics &diagnostic) {
    const auto reason = diagnostic.failure(-EINVAL);
    assert(reason.starts_with("authentication_failed_or_cancelled"));
    assert(reason.size() < 256);
    for (const auto *secret : {"private-fixture-password", "private-server-text", "123456", "fixture-user"})
        assert(reason.find(secret) == std::string::npos);
}
int main() {
    std::ostringstream events;
    auto *output = std::cout.rdbuf(events.rdbuf());
    AuthState state;
    state.protocol = "gp";
    oc_form_opt password{}, user{};
    password.type = OC_FORM_OPT_PASSWORD;
    password.name = const_cast<char *>("passwd");
    password.label = const_cast<char *>("Password:");
    user.type = OC_FORM_OPT_TEXT;
    user.name = const_cast<char *>("user");
    user.label = const_cast<char *>("Username:");
    user.next = &password;
    oc_auth_form form{};
    form.opts = &user;
    form.message = const_cast<char *>("private-server-text");
    Json::Value values;
    values["user"] = "fixture-user";
    values["passwd"] = "private-fixture-password";
    answer(values);
    assert(auth(&state, &form) == OC_FORM_RESULT_OK);
    assert(state.diagnostic.callback == AuthCallback::accepted);
    assert(state.diagnostic.forms == 1 && state.diagnostic.replies == 1);
    secret_free(state.diagnostic);

    form.opts = &password;
    password.label = const_cast<char *>("Challenge:");
    Json::Value otp;
    otp["passwd"] = "123456";
    answer(otp);
    assert(auth(&state, &form) == OC_FORM_RESULT_OK);
    assert(state.diagnostic.forms == 2 && state.diagnostic.replies == 2);
    secret_free(state.diagnostic);

    answer(otp, challenge + 100);
    assert(auth(&state, &form) == OC_FORM_RESULT_CANCELLED);
    assert(state.diagnostic.callback == AuthCallback::stale_answer);
    Json::Value invalid;
    invalid["passwd"] = 123456;
    answer(invalid);
    assert(auth(&state, &form) == OC_FORM_RESULT_ERR);
    assert(state.diagnostic.callback == AuthCallback::invalid_answer);
    input("{invalid private-fixture-password}\n");
    assert(auth(&state, &form) == OC_FORM_RESULT_CANCELLED);
    assert(state.diagnostic.callback == AuthCallback::invalid_channel);
    input("");
    assert(auth(&state, &form) == OC_FORM_RESULT_CANCELLED);
    assert(state.diagnostic.callback == AuthCallback::channel_closed);
    secret_free(state.diagnostic);
    assert(browser_auth(nullptr, "javascript:private-fixture-password", &state) == -1);
    assert(state.diagnostic.callback == AuthCallback::invalid_sso);
    assert(reject_cert(&state, "private-server-text") == -1);
    assert(state.diagnostic.callback == AuthCallback::certificate_rejected);
    secret_free(state.diagnostic);
    state.diagnostic.callback = static_cast<AuthCallback>(999);
    state.diagnostic.stage = static_cast<AuthStage>(999);
    state.diagnostic.forms = state.diagnostic.replies = UINT_MAX;
    assert(state.diagnostic.failure(INT_MIN).size() < 256);
    assert(state.diagnostic.failure(INT_MIN).find("unknown") != std::string::npos);
    std::free(user._value);
    std::free(password._value);
    std::cout.rdbuf(output);
    std::cout << "PASS: local credentials, OTP, stale/malformed/closed input, TLS/SSO and secret-free bounded diagnostics\n";
}
