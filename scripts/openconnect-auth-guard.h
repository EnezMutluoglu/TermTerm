/* Diagnostic CLI only: decide from protocol state, never from server log text. */
static const char *termterm_auth_host;
static int termterm_auth_port;
static int termterm_auth_submitted;
static int termterm_auth_exit;

enum termterm_auth_result {
    TERMTERM_AUTH_INITIAL, TERMTERM_AUTH_OTP, TERMTERM_AUTH_STOP,
    TERMTERM_AUTH_ORIGIN, TERMTERM_AUTH_UNEXPECTED
};

static enum termterm_auth_result termterm_auth_check(const char *host, int port,
                                                    const struct oc_auth_form *form)
{
    const struct oc_form_opt *opt;
    int username = 0, password = 0, code = 0, count = 0;
    if (!host || !termterm_auth_host || strcasecmp(host, termterm_auth_host) || port != termterm_auth_port)
        return TERMTERM_AUTH_ORIGIN;
    for (opt = form->opts; opt; opt = opt->next) {
        if (opt->type == OC_FORM_OPT_HIDDEN || opt->flags & OC_FORM_OPT_IGNORE)
            continue;
        count++;
        if (!opt->name)
            return TERMTERM_AUTH_UNEXPECTED;
        if (!strcmp(opt->name, "username") && opt->type == OC_FORM_OPT_TEXT) username++;
        if (!strcmp(opt->name, "credential") && opt->type == OC_FORM_OPT_PASSWORD) password++;
        if (!strcmp(opt->name, "code") &&
            (opt->type == OC_FORM_OPT_PASSWORD || opt->type == OC_FORM_OPT_TOKEN)) code++;
    }
    if (termterm_auth_submitted)
        return form->auth_id && !strcmp(form->auth_id, "_challenge") && count == 1 && code == 1
            ? TERMTERM_AUTH_OTP : TERMTERM_AUTH_STOP;
    if (!form->auth_id || strcmp(form->auth_id, "_login") || form->authgroup_opt ||
        (form->error && form->error[0]) || count != 2 || username != 1 || password != 1)
        return TERMTERM_AUTH_UNEXPECTED;
    return TERMTERM_AUTH_INITIAL;
}
