#!/bin/bash
set -e

echo "🔧 Setting up SMART branch protection (no self-approval BS)..."

# Create protection with CI-only requirements
cat > /tmp/smart_protection.json << 'EOF'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Test Suite (stable)",
      "Clippy",
      "Rustfmt",
      "Security Audit"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_linear_history": true,
  "allow_auto_merge": true,
  "required_conversation_resolution": false,
  "lock_branch": false,
  "allow_fork_syncing": true
}
EOF

# Apply the smarter protection
gh api repos/fabracht/mqtt-lib/branches/main/protection \
  --method PUT \
  --input /tmp/smart_protection.json \
  --silent

rm /tmp/smart_protection.json

echo "✅ Smart branch protection configured!"
echo ""
echo "New rules:"
echo "  ✓ CI must pass (Test, Clippy, Fmt, Security)"
echo "  ✓ Auto-merge enabled"
echo "  ✓ Linear history required"
echo "  ✗ NO review requirement (you can merge directly)"
echo "  ✓ Contributors still need PRs (enforced by GitHub)"
echo ""
echo "Your workflow:"
echo "  1. Create PR → CI runs"
echo "  2. If CI passes → gh pr merge --auto --squash"
echo "  3. No stupid self-approval required!"