#!/bin/bash
set -e

echo "🔧 Setting up complete branch protection for mqtt-lib..."

# Create a proper JSON payload
cat > /tmp/protection.json << 'EOF'
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
  "required_pull_request_reviews": {
    "required_approving_review_count": 1,
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "require_last_push_approval": false
  },
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

# Apply the protection
echo "Applying protection rules..."
if gh api repos/fabracht/mqtt-lib/branches/main/protection \
  --method PUT \
  --input /tmp/protection.json \
  --silent; then
  echo "✅ Branch protection successfully configured!"
else
  echo "❌ Failed to apply branch protection. Trying simplified version..."
  
  # Fallback to simpler protection
  cat > /tmp/protection_simple.json << 'EOF'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["Test Suite (stable)", "Clippy", "Rustfmt"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "required_approving_review_count": 1,
    "dismiss_stale_reviews": true
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
EOF
  
  gh api repos/fabracht/mqtt-lib/branches/main/protection \
    --method PUT \
    --input /tmp/protection_simple.json \
    --silent
  
  echo "✅ Applied simplified protection"
fi

# Clean up
rm -f /tmp/protection.json /tmp/protection_simple.json

# Create labels
echo "Setting up labels..."
gh label create "auto-merge" --description "Enable auto-merge when CI passes" --color "0e8a16" 2>/dev/null || true
gh label create "dependencies" --description "Dependency updates" --color "0366d6" 2>/dev/null || true
gh label create "ci-passed" --description "All CI checks have passed" --color "28a745" 2>/dev/null || true

echo ""
echo "🎉 Branch protection is now active!"
echo ""
echo "Current settings:"
echo "  ✓ PRs required for all changes"
echo "  ✓ CI checks must pass"
echo "  ✓ 1 review required"
echo "  ✓ Stale reviews dismissed"
echo "  ✓ You can bypass as admin"
echo ""
echo "Quick commands:"
echo "  • Create PR:     gh pr create --fill"
echo "  • Self-approve:  gh pr review --approve"  
echo "  • Auto-merge:    gh pr merge --auto --squash"