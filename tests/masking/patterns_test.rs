use ecotokens::masking::mask;

#[test]
fn clean_text_unchanged() {
    let (out, redacted) = mask("hello world");
    assert_eq!(out, "hello world");
    assert!(!redacted);
}

// ── Cloud ────────────────────────────────────────────────────────────────────

#[test]
fn aws_key_is_masked() {
    let (out, redacted) = mask("key=AKIAIOSFODNN7EXAMPLE");
    assert!(out.contains("[AWS_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn aws_key_asia_is_masked() {
    let (out, redacted) = mask("key=ASIAIOSFODNN7EXAMPLE");
    assert!(out.contains("[AWS_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn gcp_api_key_is_masked() {
    let (out, redacted) = mask("key=AIzaSyD-9tSrke72I6e0DVblZUMwhAqHNFQ4Cxg");
    assert!(out.contains("[GCP_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn azure_ad_client_secret_is_masked() {
    // Avoid "secret=" prefix which triggers the generic env rule first.
    let (out, redacted) = mask("value=abc1Q~abcdefghijklmnopqrstuvwxyz1234567");
    assert!(out.contains("[AZURE_SECRET]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn digitalocean_pat_is_masked() {
    let token = format!("dop_v1_{}", "a".repeat(64));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[DIGITALOCEAN_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn digitalocean_access_token_is_masked() {
    let token = format!("doo_v1_{}", "b".repeat(64));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[DIGITALOCEAN_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── AI APIs ──────────────────────────────────────────────────────────────────

#[test]
fn anthropic_api_key_is_masked() {
    // Build dynamically to avoid a literal secret in source.
    let key = format!("sk-ant-api03-{}AA", "x".repeat(93));
    let (out, redacted) = mask(&key);
    assert!(out.contains("[ANTHROPIC_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn anthropic_admin_api_key_is_masked() {
    let key = format!("sk-ant-admin01-{}AA", "x".repeat(93));
    let (out, redacted) = mask(&key);
    assert!(out.contains("[ANTHROPIC_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn openai_api_key_legacy_is_masked() {
    let key = format!("sk-{}T3BlbkFJ{}", "a".repeat(20), "b".repeat(20));
    let (out, redacted) = mask(&key);
    assert!(out.contains("[OPENAI_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn huggingface_token_is_masked() {
    let token = format!("hf_{}", "a".repeat(34));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[HF_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── VCS ──────────────────────────────────────────────────────────────────────

#[test]
fn github_pat_is_masked() {
    let (out, redacted) = mask("token: ghp_abcdefghijklmnopqrstuvwxyzABCDEFGH");
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn github_fine_grained_pat_is_masked() {
    let token = format!("github_pat_{}", "a".repeat(82));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn github_app_token_is_masked() {
    let (out, redacted) = mask("ghu_abcdefghijklmnopqrstuvwxyzABCDEFGH12");
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn github_oauth_token_is_masked() {
    let (out, redacted) = mask("gho_abcdefghijklmnopqrstuvwxyzABCDEFGH12");
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn github_refresh_token_is_masked() {
    let (out, redacted) = mask("ghr_abcdefghijklmnopqrstuvwxyzABCDEFGH12");
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn gitlab_pat_is_masked() {
    let (out, redacted) = mask("token: glpat-abcdefghij1234567890");
    assert!(out.contains("[GITLAB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn gitlab_deploy_token_is_masked() {
    let (out, redacted) = mask("token: gldt-abcdefghij1234567890");
    assert!(out.contains("[GITLAB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── Communication ────────────────────────────────────────────────────────────

#[test]
fn slack_token_is_masked() {
    let (out, redacted) = mask("hook_url: xoxb-1234567890-abcdefghijkl");
    assert!(out.contains("[SLACK_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn slack_app_token_is_masked() {
    let (out, redacted) = mask("xapp-1-A1B2C3D4E5-1234567890-abcdef1234");
    assert!(out.contains("[SLACK_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn twilio_api_key_is_masked() {
    let key = format!("SK{}", "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
    let (out, redacted) = mask(&key);
    assert!(out.contains("[TWILIO_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn sendgrid_token_is_masked() {
    let token = format!("SG.{}", "a".repeat(66));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[SENDGRID_KEY]"), "got: {out}");
    assert!(redacted);
}

// ── Dev tooling ──────────────────────────────────────────────────────────────

#[test]
fn npm_token_is_masked() {
    let token = format!("npm_{}", "a".repeat(36));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[NPM_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn pypi_token_is_masked() {
    let token = format!("pypi-AgEIcHlwaS5vcmc{}", "x".repeat(55));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[PYPI_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn databricks_token_is_masked() {
    let token = format!("dapi{}", "a1b2".repeat(8));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[DATABRICKS_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn hashicorp_tf_token_is_masked() {
    let token = format!("{}.atlasv1.{}", "a".repeat(14), "b".repeat(65));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[TF_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn pulumi_token_is_masked() {
    let token = format!("pul-{}", "a1b2c3d4".repeat(5));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[PULUMI_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn postman_token_is_masked() {
    let token = format!(
        "PMAK-{}-{}",
        "a1b2c3d4e5f6a1b2c3d4e5f6", "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5"
    );
    let (out, redacted) = mask(&token);
    assert!(out.contains("[POSTMAN_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── Observability ────────────────────────────────────────────────────────────

#[test]
fn grafana_api_key_is_masked() {
    let key = format!("eyJrIjoi{}", "A".repeat(75));
    let (out, redacted) = mask(&key);
    assert!(out.contains("[GRAFANA_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn grafana_cloud_token_is_masked() {
    let token = format!("glc_{}", "A".repeat(40));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[GRAFANA_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn grafana_service_account_token_is_masked() {
    let token = format!("glsa_{}_1a2b3c4d", "a".repeat(32));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[GRAFANA_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn sentry_user_token_is_masked() {
    let token = format!("sntryu_{}", "a1b2".repeat(16));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[SENTRY_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── Payment ──────────────────────────────────────────────────────────────────

#[test]
fn stripe_key_is_masked() {
    // Build the key dynamically so it is not a literal secret in source.
    let key = format!("sk_{}_abcdefghijklmnopqrstuvwx", "live");
    let (out, redacted) = mask(&format!("key: {key}"));
    assert!(out.contains("[STRIPE_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn stripe_rk_prod_key_is_masked() {
    let key = format!("rk_prod_abcdefghijklmnopqrstuvwx");
    let (out, redacted) = mask(&key);
    assert!(out.contains("[STRIPE_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn shopify_access_token_is_masked() {
    let token = format!("shpat_{}", "a1b2c3d4".repeat(4));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[SHOPIFY_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn shopify_shared_secret_is_masked() {
    let token = format!("shpss_{}", "a1b2c3d4".repeat(4));
    let (out, redacted) = mask(&token);
    assert!(out.contains("[SHOPIFY_TOKEN]"), "got: {out}");
    assert!(redacted);
}

// ── Generic ──────────────────────────────────────────────────────────────────

#[test]
fn bearer_token_is_masked() {
    let (out, redacted) = mask("Authorization: Bearer eyJhbGciOiJSUzI1NiJ9");
    assert!(out.contains("[BEARER_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn pem_private_key_is_masked() {
    let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
    let (out, redacted) = mask(input);
    assert!(out.contains("[PRIVATE_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn env_secret_variable_is_masked() {
    let (out, redacted) = mask("API_KEY=supersecretvalue123");
    assert!(out.contains("[REDACTED]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn env_password_variable_is_masked() {
    let (out, redacted) = mask("PASSWORD=hunter2");
    assert!(out.contains("[REDACTED]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn jwt_is_masked() {
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let (out, redacted) = mask(jwt);
    assert!(out.contains("[JWT_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn url_with_credentials_is_masked() {
    let (out, redacted) = mask("https://user:password@github.com/repo.git");
    assert!(out.contains("[CREDENTIALS]"), "got: {out}");
    assert!(redacted);
}
