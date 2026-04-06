use lazy_regex::regex;

/// Apply all secret-masking patterns to `text`.
/// Returns (masked_text, was_redacted).
pub fn mask(text: &str) -> (String, bool) {
    let mut out = text.to_string();
    let mut redacted = false;

    // ── Cloud ────────────────────────────────────────────────────────────────

    // AWS Access Key ID (AKIA, ASIA, ABIA, ACCA, A3T…)
    let re_aws = regex!(r"(?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16}");
    if re_aws.is_match(&out) {
        out = re_aws.replace_all(&out, "[AWS_KEY]").into_owned();
        redacted = true;
    }

    // GCP API Key
    let re_gcp = regex!(r"AIza[\w\-]{35}");
    if re_gcp.is_match(&out) {
        out = re_gcp.replace_all(&out, "[GCP_KEY]").into_owned();
        redacted = true;
    }

    // Azure AD Client Secret
    let re_azure = regex!(r"[a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.\-]{31,34}");
    if re_azure.is_match(&out) {
        out = re_azure.replace_all(&out, "[AZURE_SECRET]").into_owned();
        redacted = true;
    }

    // DigitalOcean Personal Access Token
    let re_do_pat = regex!(r"dop_v1_[a-f0-9]{64}");
    if re_do_pat.is_match(&out) {
        out = re_do_pat
            .replace_all(&out, "[DIGITALOCEAN_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // DigitalOcean OAuth Access Token
    let re_do_oauth = regex!(r"doo_v1_[a-f0-9]{64}");
    if re_do_oauth.is_match(&out) {
        out = re_do_oauth
            .replace_all(&out, "[DIGITALOCEAN_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // ── AI APIs ──────────────────────────────────────────────────────────────

    // Anthropic API Key
    let re_anthropic = regex!(r"sk-ant-api03-[a-zA-Z0-9_\-]{93}AA");
    if re_anthropic.is_match(&out) {
        out = re_anthropic
            .replace_all(&out, "[ANTHROPIC_KEY]")
            .into_owned();
        redacted = true;
    }

    // Anthropic Admin API Key
    let re_anthropic_admin = regex!(r"sk-ant-admin01-[a-zA-Z0-9_\-]{93}AA");
    if re_anthropic_admin.is_match(&out) {
        out = re_anthropic_admin
            .replace_all(&out, "[ANTHROPIC_KEY]")
            .into_owned();
        redacted = true;
    }

    // OpenAI API Key
    let re_openai = regex!(
        r"sk-(?:proj|svcacct|admin)-[A-Za-z0-9_\-]{58,74}T3BlbkFJ[A-Za-z0-9_\-]{58,74}|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20}"
    );
    if re_openai.is_match(&out) {
        out = re_openai.replace_all(&out, "[OPENAI_KEY]").into_owned();
        redacted = true;
    }

    // HuggingFace Access Token
    let re_hf = regex!(r"hf_[a-zA-Z0-9_]{34}");
    if re_hf.is_match(&out) {
        out = re_hf.replace_all(&out, "[HF_TOKEN]").into_owned();
        redacted = true;
    }

    // ── VCS ──────────────────────────────────────────────────────────────────

    // GitHub PAT (classic)
    let re_ghp = regex!(r"ghp_[A-Za-z0-9_]{32,}");
    if re_ghp.is_match(&out) {
        out = re_ghp.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitHub Fine-Grained PAT
    let re_gh_fgpat = regex!(r"github_pat_\w{82}");
    if re_gh_fgpat.is_match(&out) {
        out = re_gh_fgpat.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitHub App / Installation Token (ghu_, ghs_)
    let re_gh_app = regex!(r"(?:ghu|ghs)_[0-9a-zA-Z]{36}");
    if re_gh_app.is_match(&out) {
        out = re_gh_app.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitHub OAuth Token
    let re_gho = regex!(r"gho_[0-9a-zA-Z]{36}");
    if re_gho.is_match(&out) {
        out = re_gho.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitHub Refresh Token
    let re_ghr = regex!(r"ghr_[0-9a-zA-Z]{36}");
    if re_ghr.is_match(&out) {
        out = re_ghr.replace_all(&out, "[GITHUB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitLab Personal Access Token
    let re_glpat = regex!(r"glpat-[\w\-]{20}");
    if re_glpat.is_match(&out) {
        out = re_glpat.replace_all(&out, "[GITLAB_TOKEN]").into_owned();
        redacted = true;
    }

    // GitLab Deploy Token
    let re_gldt = regex!(r"gldt-[0-9a-zA-Z_\-]{20}");
    if re_gldt.is_match(&out) {
        out = re_gldt.replace_all(&out, "[GITLAB_TOKEN]").into_owned();
        redacted = true;
    }

    // ── Communication ────────────────────────────────────────────────────────

    // Slack tokens (bot, user, app legacy)
    let re_slack = regex!(r"xox[bpoa]-[A-Za-z0-9-]{10,}");
    if re_slack.is_match(&out) {
        out = re_slack.replace_all(&out, "[SLACK_TOKEN]").into_owned();
        redacted = true;
    }

    // Slack App-Level Token (xapp-)
    let re_slack_app = regex!(r"(?i)xapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+");
    if re_slack_app.is_match(&out) {
        out = re_slack_app.replace_all(&out, "[SLACK_TOKEN]").into_owned();
        redacted = true;
    }

    // Twilio API Key
    let re_twilio = regex!(r"SK[0-9a-fA-F]{32}");
    if re_twilio.is_match(&out) {
        out = re_twilio.replace_all(&out, "[TWILIO_KEY]").into_owned();
        redacted = true;
    }

    // SendGrid API Token
    let re_sendgrid = regex!(r"SG\.[a-zA-Z0-9=_\-.]{66}");
    if re_sendgrid.is_match(&out) {
        out = re_sendgrid.replace_all(&out, "[SENDGRID_KEY]").into_owned();
        redacted = true;
    }

    // ── Dev tooling ──────────────────────────────────────────────────────────

    // npm Access Token
    let re_npm = regex!(r"npm_[a-zA-Z0-9]{36}");
    if re_npm.is_match(&out) {
        out = re_npm.replace_all(&out, "[NPM_TOKEN]").into_owned();
        redacted = true;
    }

    // PyPI Upload Token
    let re_pypi = regex!(r"pypi-AgEIcHlwaS5vcmc[\w\-]{50,200}");
    if re_pypi.is_match(&out) {
        out = re_pypi.replace_all(&out, "[PYPI_TOKEN]").into_owned();
        redacted = true;
    }

    // Databricks API Token
    let re_databricks = regex!(r"dapi[a-f0-9]{32}(?:-\d)?");
    if re_databricks.is_match(&out) {
        out = re_databricks
            .replace_all(&out, "[DATABRICKS_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // HashiCorp Terraform Cloud API Token
    let re_tf = regex!(r"[a-zA-Z0-9]{14}\.atlasv1\.[a-zA-Z0-9\-_=]{60,70}");
    if re_tf.is_match(&out) {
        out = re_tf.replace_all(&out, "[TF_TOKEN]").into_owned();
        redacted = true;
    }

    // Pulumi API Token
    let re_pulumi = regex!(r"pul-[a-f0-9]{40}");
    if re_pulumi.is_match(&out) {
        out = re_pulumi.replace_all(&out, "[PULUMI_TOKEN]").into_owned();
        redacted = true;
    }

    // Postman API Token
    let re_postman = regex!(r"PMAK-[a-fA-F0-9]{24}-[a-fA-F0-9]{34}");
    if re_postman.is_match(&out) {
        out = re_postman.replace_all(&out, "[POSTMAN_TOKEN]").into_owned();
        redacted = true;
    }

    // ── Observability ────────────────────────────────────────────────────────

    // Grafana API Key (legacy, starts with eyJrIjoi in base64)
    let re_grafana_key = regex!(r"eyJrIjoi[A-Za-z0-9+/]{70,150}={0,3}");
    if re_grafana_key.is_match(&out) {
        out = re_grafana_key
            .replace_all(&out, "[GRAFANA_KEY]")
            .into_owned();
        redacted = true;
    }

    // Grafana Cloud API Token
    let re_grafana_cloud = regex!(r"glc_[A-Za-z0-9+/]{32,150}={0,3}");
    if re_grafana_cloud.is_match(&out) {
        out = re_grafana_cloud
            .replace_all(&out, "[GRAFANA_KEY]")
            .into_owned();
        redacted = true;
    }

    // Grafana Service Account Token
    let re_grafana_sa = regex!(r"glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8}");
    if re_grafana_sa.is_match(&out) {
        out = re_grafana_sa
            .replace_all(&out, "[GRAFANA_KEY]")
            .into_owned();
        redacted = true;
    }

    // Sentry User Auth Token
    let re_sentry_user = regex!(r"sntryu_[a-f0-9]{64}");
    if re_sentry_user.is_match(&out) {
        out = re_sentry_user
            .replace_all(&out, "[SENTRY_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // Sentry Org Auth Token
    let re_sentry_org = regex!(
        r"sntrys_eyJpYXQiO[a-zA-Z0-9+/]{10,200}(?:LCJyZWdpb25fdXJs|InJlZ2lvbl91cmwi|cmVnaW9uX3VybCI6)[a-zA-Z0-9+/]{10,200}={0,2}_[a-zA-Z0-9+/]{43}"
    );
    if re_sentry_org.is_match(&out) {
        out = re_sentry_org
            .replace_all(&out, "[SENTRY_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // ── Payment ──────────────────────────────────────────────────────────────

    // Stripe keys (sk_ and rk_, test/live/prod)
    let re_stripe = regex!(r"(?:sk|rk)_(?:test|live|prod)_[a-zA-Z0-9]{10,99}");
    if re_stripe.is_match(&out) {
        out = re_stripe.replace_all(&out, "[STRIPE_KEY]").into_owned();
        redacted = true;
    }

    // Shopify Access Token
    let re_shopify_pat = regex!(r"shpat_[a-fA-F0-9]{32}");
    if re_shopify_pat.is_match(&out) {
        out = re_shopify_pat
            .replace_all(&out, "[SHOPIFY_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // Shopify Shared Secret
    let re_shopify_ss = regex!(r"shpss_[a-fA-F0-9]{32}");
    if re_shopify_ss.is_match(&out) {
        out = re_shopify_ss
            .replace_all(&out, "[SHOPIFY_TOKEN]")
            .into_owned();
        redacted = true;
    }

    // ── Generic ──────────────────────────────────────────────────────────────

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

    (out, redacted)
}
