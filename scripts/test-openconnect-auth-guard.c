#include <assert.h>
#include <string.h>
#include <strings.h>
#include <winsock2.h>
#include <openconnect.h>
#include "openconnect-auth-guard.h"

int main(void)
{
    struct oc_form_opt password = { .type = OC_FORM_OPT_PASSWORD, .name = "credential" };
    struct oc_form_opt username = { .next = &password, .type = OC_FORM_OPT_TEXT, .name = "username" };
    struct oc_auth_form form = { .auth_id = "_login", .opts = &username };
    termterm_auth_host = "vpn.example";
    termterm_auth_port = 443;
    assert(termterm_auth_check("VPN.EXAMPLE", 443, &form) == TERMTERM_AUTH_INITIAL);
    assert(termterm_auth_check("other.example", 443, &form) == TERMTERM_AUTH_ORIGIN);
    assert(termterm_auth_check("vpn.example", 444, &form) == TERMTERM_AUTH_ORIGIN);
    assert(termterm_auth_check("other.example\nUsername: ", 443, &form) == TERMTERM_AUTH_ORIGIN);
    assert(!username._value && !password._value);
    password.name = "unexpected";
    assert(termterm_auth_check("vpn.example", 443, &form) == TERMTERM_AUTH_UNEXPECTED);
    password.name = "credential";
    termterm_auth_submitted = 1;
    assert(termterm_auth_check("vpn.example", 443, &form) == TERMTERM_AUTH_STOP);
    username.type = OC_FORM_OPT_HIDDEN;
    password.type = OC_FORM_OPT_PASSWORD;
    password.name = "code";
    form.auth_id = "_challenge";
    assert(termterm_auth_check("vpn.example", 443, &form) == TERMTERM_AUTH_OTP);
    password.type = OC_FORM_OPT_TOKEN;
    assert(termterm_auth_check("vpn.example", 443, &form) == TERMTERM_AUTH_OTP);
    assert(termterm_auth_check("other.example", 443, &form) == TERMTERM_AUTH_ORIGIN);
    return 0;
}
