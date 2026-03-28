use lazy_regex::regex;

/// Apply all secret-masking patterns to `text`.
/// Returns (masked_text, was_redacted).
pub fn mask(text: &str) -> (String, bool) {
    let mut out = text.to_string();
    let mut redacted = false;

    // AWS Access Key ID
    let re_aws = regex!(r"AKIA[A-Z0-9]{16}");
    if re_aws.is_match(&out) {
        out = re_aws.replace_all(&out, "[AWS_KEY]").into_owned();
        redacted = true;
    }

    // GitHub PAT (ghp_...)
    let re_ghp = regex!(r"ghp_[A-Za-z0-9_]{32,}");
    if re_ghp.is_match(&out) {
        out = re_ghp.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // Bearer token
    let re_bearer = regex!(r"(?i)Bearer\s+[A-Za-z0-9\-._~+/]+=*");
    if re_bearer.is_match(&out) {
        out = re_bearer.replace_all(&out, "[BEARER_TOKEN]").into_owned();
        redacted = true;
    }

    // PEM private key block
    let re_pem =
        regex!(r"(?s)-----BEGIN [A-Z ]* PRIVATE KEY-----.*?-----END [A-Z ]* PRIVATE KEY-----");
    if re_pem.is_match(&out) {
        out = re_pem.replace_all(&out, "[PRIVATE_KEY]").into_owned();
        redacted = true;
    }

    // .env secrets: NAME=value where NAME hints at a secret
    let re_env = regex!(r"(?i)(SECRET|PASSWORD|API_KEY|PASSWD|TOKEN|PRIVATE_KEY|AUTH)=[^\s\n]+");
    if re_env.is_match(&out) {
        out = re_env.replace_all(&out, "[REDACTED]").into_owned();
        redacted = true;
    }

    // JWT (three base64url segments)
    let re_jwt = regex!(r"ey[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}");
    if re_jwt.is_match(&out) {
        out = re_jwt.replace_all(&out, "[JWT_TOKEN]").into_owned();
        redacted = true;
    }

    // URL with embedded credentials: https://user:pass@host
    let re_url = regex!(r"(https?://)([^:@\s]+:[^@\s]+)@");
    if re_url.is_match(&out) {
        out = re_url.replace_all(&out, "${1}[CREDENTIALS]@").into_owned();
        redacted = true;
    }

    // Slack tokens
    let re_slack = regex!(r"xox[bpoa]-[A-Za-z0-9-]{10,}");
    if re_slack.is_match(&out) {
        out = re_slack.replace_all(&out, "[SLACK_TOKEN]").into_owned();
        redacted = true;
    }

    // Stripe secret keys
    let re_stripe = regex!(r"sk_(live|test)_[A-Za-z0-9]{24,}");
    if re_stripe.is_match(&out) {
        out = re_stripe.replace_all(&out, "[STRIPE_KEY]").into_owned();
        redacted = true;
    }

    (out, redacted)
}
