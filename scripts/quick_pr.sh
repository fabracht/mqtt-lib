#!/bin/bash
# Quick PR creation and auto-merge setup for AI-assisted development

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Get current branch
CURRENT_BRANCH=$(git branch --show-current)

if [ "$CURRENT_BRANCH" = "main" ] || [ "$CURRENT_BRANCH" = "master" ]; then
    echo -e "${YELLOW}⚠️  You're on the main branch. Creating feature branch...${NC}"
    read -p "Branch name (e.g., feature/add-something): " BRANCH_NAME
    git checkout -b "$BRANCH_NAME"
    CURRENT_BRANCH="$BRANCH_NAME"
fi

echo -e "${BLUE}📝 Creating PR for branch: ${CURRENT_BRANCH}${NC}"

# Push current branch
echo "Pushing branch..."
git push -u origin "$CURRENT_BRANCH"

# Create PR with AI-friendly template
PR_URL=$(gh pr create \
  --title "$CURRENT_BRANCH" \
  --body "## Changes

This PR was created with AI assistance.

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Tests pass locally
- [ ] Clippy passes
- [ ] Formatted with rustfmt

## Notes
_AI pair programming session_" \
  --draft=false \
  --web=false)

echo -e "${GREEN}✅ PR created: ${PR_URL}${NC}"

# Extract PR number
PR_NUMBER=$(echo "$PR_URL" | grep -o '[0-9]*$')

# Quick approve your own PR
echo "Auto-approving PR..."
gh pr review "$PR_NUMBER" --approve --body "Self-approved (AI-assisted development)"

# Enable auto-merge
echo "Enabling auto-merge..."
gh pr merge "$PR_NUMBER" --auto --squash --delete-branch

echo -e "${GREEN}🚀 Done!${NC}"
echo ""
echo "PR #${PR_NUMBER} will auto-merge when CI passes."
echo "Check status: gh pr checks ${PR_NUMBER}"
echo "Watch CI:     gh run watch"