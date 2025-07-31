#!/bin/bash
set -e

echo "🔧 Setting up branch protection for mqtt-lib..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}This script will configure branch protection with:${NC}"
echo "  ✓ Require 1 review (but you can approve your own PRs)"
echo "  ✓ Require CI checks to pass"
echo "  ✓ Auto-merge enabled"
echo "  ✓ You keep admin override powers"
echo ""

# Get the default branch
DEFAULT_BRANCH=$(gh repo view --json defaultBranchRef --jq .defaultBranchRef.name)
echo -e "${GREEN}Default branch detected: ${DEFAULT_BRANCH}${NC}"

# Configure branch protection
echo "Configuring branch protection..."
gh api "repos/fabracht/mqtt-lib/branches/${DEFAULT_BRANCH}/protection" \
  --method PUT \
  --field required_status_checks='{
    "strict": true,
    "contexts": [
      "Rust CI / test",
      "Rust CI / clippy", 
      "Rust CI / fmt",
      "Rust CI / Security Audit"
    ]
  }' \
  --field enforce_admins=false \
  --field required_pull_request_reviews='{
    "required_approving_review_count": 1,
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "require_last_push_approval": false,
    "bypass_pull_request_allowances": {
      "users": [],
      "teams": [],
      "apps": []
    }
  }' \
  --field restrictions=null \
  --field allow_force_pushes=false \
  --field allow_deletions=false \
  --field required_linear_history=true \
  --field allow_auto_merge=true \
  --field required_conversation_resolution=false \
  --field lock_branch=false \
  --field allow_fork_syncing=true

echo -e "${GREEN}✅ Branch protection configured!${NC}"

# Create some useful labels
echo "Creating useful labels for PR management..."
gh label create "auto-merge" --description "Enable auto-merge when CI passes" --color "0e8a16" 2>/dev/null || true
gh label create "dependencies" --description "Dependency updates" --color "0366d6" 2>/dev/null || true
gh label create "rust" --description "Rust code changes" --color "dea584" 2>/dev/null || true

echo -e "${GREEN}✅ Labels created!${NC}"

echo ""
echo -e "${GREEN}🎉 Setup complete!${NC}"
echo ""
echo "Your workflow:"
echo "1. For YOUR PRs: Create PR → CI runs → You approve → Auto-merges"
echo "2. For Dependabot: PR created → You review → Approve → Auto-merges"
echo "3. For Contributors: PR created → CI must pass → You review → Add 'auto-merge' label → Merges"
echo ""
echo "Quick commands:"
echo "  Create PR:     gh pr create --fill"
echo "  Quick approve: gh pr review --approve"
echo "  Auto-merge:    gh pr merge --auto --squash"