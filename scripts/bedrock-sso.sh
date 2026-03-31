#!/usr/bin/env bash
# bedrock-sso.sh — Export AWS SSO credentials for Bedrock
#
# Usage:
#   source scripts/bedrock-sso.sh [profile]
#
# This script:
#   1. Logs in via AWS SSO (if not already authenticated)
#   2. Exports AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN
#   3. Sets OX_LLM__PROVIDER=bedrock and OX_LLM__REGION
#
# Prerequisites:
#   - AWS CLI v2 installed
#   - SSO profile configured in ~/.aws/config
#
# Example ~/.aws/config:
#   [profile my-bedrock]
#   sso_session = my-sso
#   sso_account_id = 123456789012
#   sso_role_name = BedrockUser
#   region = us-east-1
#
#   [sso-session my-sso]
#   sso_start_url = https://my-org.awsapps.com/start
#   sso_region = us-east-1

set -euo pipefail

PROFILE="${1:-umosone-pre}"

echo "==> AWS SSO login (profile: $PROFILE)"
aws sso login --profile "$PROFILE" 2>/dev/null || true

echo "==> Exporting credentials..."
CREDS=$(aws configure export-credentials --profile "$PROFILE" --format env-no-export 2>/dev/null)

if [ -z "$CREDS" ]; then
    echo "ERROR: Failed to export credentials. Run 'aws sso login --profile $PROFILE' first."
    return 1 2>/dev/null || exit 1
fi

eval "$CREDS"
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN

# Get region from profile
REGION=$(aws configure get region --profile "$PROFILE" 2>/dev/null || echo "us-east-1")
export AWS_REGION="$REGION"

# Set Ontosyx env vars
export OX_LLM__PROVIDER=bedrock
export OX_LLM__MODEL="${OX_LLM__MODEL:-global.anthropic.claude-sonnet-4-5-20250929-v1:0}"
export OX_LLM__REGION="$REGION"

echo "==> Credentials exported successfully"
echo "    AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID:0:8}..."
echo "    AWS_REGION=$REGION"
echo "    OX_LLM__PROVIDER=bedrock"
echo "    OX_LLM__MODEL=$OX_LLM__MODEL"
echo ""
echo "Now run: cargo run -p ox-api"
