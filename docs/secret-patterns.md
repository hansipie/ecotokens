# Secret Redaction Patterns

Sensitive values are detected and replaced before any content reaches the model.

| Pattern | Replaced by |
|---|---|
| AWS Access Key (`AKIA…`, `ASIA…`, `ABIA…`, `ACCA…`, `A3T…`) | `[AWS_KEY]` |
| GCP API Key (`AIza…`) | `[GCP_KEY]` |
| Azure AD Client Secret | `[AZURE_SECRET]` |
| DigitalOcean PAT (`dop_v1_…`) / OAuth token (`doo_v1_…`) | `[DIGITALOCEAN_TOKEN]` |
| Anthropic API Key (`sk-ant-api03-…`) | `[ANTHROPIC_KEY]` |
| Anthropic Admin Key (`sk-ant-admin01-…`) | `[ANTHROPIC_KEY]` |
| OpenAI API Key (`sk-proj-…`, `sk-svcacct-…`, `sk-…T3BlbkFJ…`) | `[OPENAI_KEY]` |
| HuggingFace token (`hf_…`) | `[HF_TOKEN]` |
| GitHub PAT (`ghp_…`) | `[GITHUB_TOKEN]` |
| GitHub Fine-Grained PAT (`github_pat_…`) | `[GITHUB_TOKEN]` |
| GitHub App/Installation token (`ghu_…`, `ghs_…`) | `[GITHUB_TOKEN]` |
| GitHub OAuth token (`gho_…`) | `[GITHUB_TOKEN]` |
| GitHub Refresh token (`ghr_…`) | `[GITHUB_TOKEN]` |
| GitLab PAT (`glpat-…`) | `[GITLAB_TOKEN]` |
| GitLab Deploy token (`gldt-…`) | `[GITLAB_TOKEN]` |
| Slack token (`xox[bpoa]-…`, `xapp-…`) | `[SLACK_TOKEN]` |
| Twilio API Key (`SK…`) | `[TWILIO_KEY]` |
| SendGrid API token (`SG.…`) | `[SENDGRID_KEY]` |
| npm Access token (`npm_…`) | `[NPM_TOKEN]` |
| PyPI Upload token (`pypi-AgEI…`) | `[PYPI_TOKEN]` |
| Databricks token (`dapi…`) | `[DATABRICKS_TOKEN]` |
| HashiCorp TF Cloud token (`….atlasv1.…`) | `[TF_TOKEN]` |
| Pulumi token (`pul-…`) | `[PULUMI_TOKEN]` |
| Postman API token (`PMAK-…`) | `[POSTMAN_TOKEN]` |
| Grafana API Key / Cloud token / Service Account (`eyJrIjoi…`, `glc_…`, `glsa_…`) | `[GRAFANA_KEY]` |
| Sentry User token (`sntryu_…`) / Org token (`sntrys_…`) | `[SENTRY_TOKEN]` |
| Stripe key (`sk_…`, `rk_…` — test/live/prod) | `[STRIPE_KEY]` |
| Shopify Access token (`shpat_…`) / Shared Secret (`shpss_…`) | `[SHOPIFY_TOKEN]` |
| Bearer token | `[BEARER_TOKEN]` |
| PEM private key | `[PRIVATE_KEY]` |
| `.env` secrets (`SECRET=`, `TOKEN=`, `API_KEY=`…) | `[REDACTED]` |
| JWT token | `[JWT_TOKEN]` |
| URL credentials (`user:pass@host`) | `[CREDENTIALS]` |

The implementation lives in [`src/masking/patterns.rs`](../src/masking/patterns.rs).
