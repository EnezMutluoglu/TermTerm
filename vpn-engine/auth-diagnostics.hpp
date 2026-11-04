#pragma once
#include <string>

enum class AuthCallback {
    none, waiting, accepted, stale_answer, invalid_answer, invalid_choice,
    option_rejected, channel_closed, invalid_channel, unsupported_browser,
    invalid_sso, expired_sso, sso_library_error, certificate_rejected, callback_error
};
enum class AuthStage { none, form, sso, certificate, external_browser };
inline const char *auth_callback_name(AuthCallback value) {
    switch (value) {
#define AUTH_CASE(name) case AuthCallback::name: return #name;
    AUTH_CASE(none) AUTH_CASE(waiting) AUTH_CASE(accepted) AUTH_CASE(stale_answer)
    AUTH_CASE(invalid_answer) AUTH_CASE(invalid_choice) AUTH_CASE(option_rejected)
    AUTH_CASE(channel_closed) AUTH_CASE(invalid_channel) AUTH_CASE(unsupported_browser)
    AUTH_CASE(invalid_sso) AUTH_CASE(expired_sso) AUTH_CASE(sso_library_error)
    AUTH_CASE(certificate_rejected) AUTH_CASE(callback_error)
#undef AUTH_CASE
    }
    return "unknown";
}
inline const char *auth_stage_name(AuthStage value) {
    switch (value) {
    case AuthStage::none: return "none";
    case AuthStage::form: return "form";
    case AuthStage::sso: return "sso";
    case AuthStage::certificate: return "certificate";
    case AuthStage::external_browser: return "external_browser";
    }
    return "unknown";
}
struct AuthDiagnostics {
    AuthStage stage = AuthStage::none;
    AuthCallback callback = AuthCallback::none;
    unsigned forms = 0, replies = 0;
    int returned(int result, AuthCallback outcome) { callback = outcome; return result; }
    std::string failure(int library_result) const {
        // Only enums and numbers may leave this diagnostic; no server text or values.
        return "authentication_failed_or_cancelled result=" + std::to_string(library_result) +
            " stage=" + auth_stage_name(stage) + " callback=" + auth_callback_name(callback) +
            " forms=" + std::to_string(forms) + " replies=" + std::to_string(replies);
    }
};
