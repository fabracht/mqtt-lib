#!/bin/bash
set -e

echo "🔧 Setting up branch protection for mqtt-lib..."

# Simple JSON payload
cat > /tmp/branch_protection.json << 'EOF'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["Rust CI / test"]
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

# Apply protection
gh api repos/fabracht/mqtt-lib/branches/main/protection \
  --method PUT \
  --input /tmp/branch_protection.json \
  --silent

echo "✅ Branch protection configured!"

# Create labels
gh label create "auto-merge" --description "Enable auto-merge when CI passes" --color "0e8a16" 2>/dev/null || echo "Label auto-merge already exists"
gh label create "dependencies" --description "Dependency updates" --color "0366d6" 2>/dev/null || echo "Label dependencies already exists"

rm /tmp/branch_protection.json
echo "🎉 Setup complete!"