#!/bin/bash
set -e

echo "🔧 Setting up PROPER branch protection for mqtt-lib..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Configuring full branch protection with:${NC}"
echo "  ✓ All CI checks required (test, clippy, fmt, security)"
echo "  ✓ 1 review required (you can approve your own)"
echo "  ✓ Auto-merge enabled"
echo "  ✓ Linear history enforced"
echo "  ✓ Admin bypass allowed"
echo ""

# First, let's check what CI status checks are available
echo "Checking available status checks..."
CHECKS=$(gh api repos/fabracht/mqtt-lib/commits/main/check-runs --jq '.check_runs[].name' 2>/dev/null || echo "")

if [ -z "$CHECKS" ]; then
    echo "No CI runs found yet. Using expected check names..."
    CONTEXTS='["Rust CI / test","Rust CI / clippy","Rust CI / fmt","Rust CI / Security Audit"]'
else
    echo "Found CI checks:"
    echo "$CHECKS"
    # For now, use our expected names
    CONTEXTS='["Rust CI / test","Rust CI / clippy","Rust CI / fmt","Rust CI / Security Audit"]'
fi

# Create the protection rules with proper JSON structure
echo "Applying branch protection..."

# Use gh api with individual fields (more reliable than JSON input)
gh api \
  --method PUT \
  repos/fabracht/mqtt-lib/branches/main/protection \
  --raw-field 'required_status_checks[strict]=true' \
  --raw-field 'required_status_checks[contexts][]=Rust CI / test' \
  --raw-field 'required_status_checks[contexts][]=Rust CI / clippy' \
  --raw-field 'required_status_checks[contexts][]=Rust CI / fmt' \
  --raw-field 'required_status_checks[contexts][]=Rust CI / Security Audit' \
  --raw-field 'enforce_admins=false' \
  --raw-field 'required_pull_request_reviews[required_approving_review_count]=1' \
  --raw-field 'required_pull_request_reviews[dismiss_stale_reviews]=true' \
  --raw-field 'required_pull_request_reviews[require_code_owner_reviews]=false' \
  --raw-field 'required_pull_request_reviews[require_last_push_approval]=false' \
  --field 'restrictions=null' \
  --raw-field 'allow_force_pushes=false' \
  --raw-field 'allow_deletions=false' \
  --raw-field 'required_linear_history=true' \
  --raw-field 'allow_auto_merge=true' \
  --raw-field 'required_conversation_resolution=false' \
  --raw-field 'lock_branch=false' \
  --raw-field 'allow_fork_syncing=true' \
  --silent

echo -e "${GREEN}✅ Branch protection configured successfully!${NC}"

# Create useful labels
echo "Creating workflow labels..."
gh label create "auto-merge" --description "Enable auto-merge when CI passes" --color "0e8a16" 2>/dev/null || echo "  • Label 'auto-merge' already exists"
gh label create "dependencies" --description "Dependency updates" --color "0366d6" 2>/dev/null || echo "  • Label 'dependencies' already exists"
gh label create "rust" --description "Rust code changes" --color "dea584" 2>/dev/null || echo "  • Label 'rust' already exists"
gh label create "ci-ready" --description "CI checks have passed" --color "28a745" 2>/dev/null || echo "  • Label 'ci-ready' already exists"

echo ""
echo -e "${GREEN}🎉 Full branch protection is now active!${NC}"
echo ""
echo "Protection summary:"
echo "  • PR required for all changes to main"
echo "  • Must pass: test, clippy, fmt, security audit"
echo "  • 1 approval required (can self-approve)"
echo "  • Enforces linear history (no merge commits)"
echo "  • Auto-merge is enabled for the repo"
echo ""
echo "Workflow reminders:"
echo "  1. Your PRs: ./scripts/quick_pr.sh → auto-approves → merges when CI passes"
echo "  2. Contributors: PR → CI must pass → you review → add 'auto-merge' label"
echo "  3. Dependabot: PR → you review → approve → auto-merges"