use ecotokens::masking::mask;

#[test]
fn clean_text_unchanged() {
    let (out, redacted) = mask("hello world");
    assert_eq!(out, "hello world");
    assert!(!redacted);
}

#[test]
fn aws_key_is_masked() {
    let (out, redacted) = mask("key=AKIAIOSFODNN7EXAMPLE");
    assert!(out.contains("[AWS_KEY]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn github_pat_is_masked() {
    let (out, redacted) = mask("token: ghp_abcdefghijklmnopqrstuvwxyzABCDEFGH");
    assert!(out.contains("[GITHUB_TOKEN]"), "got: {out}");
    assert!(redacted);
}

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

#[test]
fn slack_token_is_masked() {
    let (out, redacted) = mask("hook_url: xoxb-1234567890-abcdefghijkl");
    assert!(out.contains("[SLACK_TOKEN]"), "got: {out}");
    assert!(redacted);
}

#[test]
fn stripe_key_is_masked() {
    let (out, redacted) = mask("key: sk_LIVE_XXXXXXXXXXXXXXXXXXXXXXXX");
    assert!(out.contains("[STRIPE_KEY]"), "got: {out}");
    assert!(redacted);
}
