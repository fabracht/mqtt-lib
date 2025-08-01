#!/bin/bash
#
# Script to install git hooks for the project
#

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
HOOKS_DIR="$REPO_ROOT/.githooks"

# Color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Installing git hooks...${NC}"

# Create .githooks directory if it doesn't exist
mkdir -p "$HOOKS_DIR"

# Create the pre-commit hook
cat > "$HOOKS_DIR/pre-commit" << 'EOF'
#!/bin/bash
#
# Pre-commit hook that runs cargo make ci-verify
# This ensures all CI checks pass before allowing a commit
#

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Running pre-commit CI verification...${NC}"
echo "This ensures your code will pass CI before committing."
echo ""

# Check if cargo-make is installed
if ! command -v cargo-make &> /dev/null; then
    echo -e "${RED}Error: cargo-make is not installed${NC}"
    echo "Please install it with: cargo install cargo-make"
    exit 1
fi

# Run the CI verification
if cargo make ci-verify; then
    echo ""
    echo -e "${GREEN}✓ All CI checks passed!${NC}"
    echo "Proceeding with commit..."
    exit 0
else
    echo ""
    echo -e "${RED}✗ CI verification failed!${NC}"
    echo "Please fix the issues above before committing."
    echo ""
    echo "To bypass this check (not recommended), use:"
    echo "  git commit --no-verify"
    exit 1
fi
EOF

# Make the hook executable
chmod +x "$HOOKS_DIR/pre-commit"

# Configure git to use the hooks directory
git config core.hooksPath "$HOOKS_DIR"

echo -e "${GREEN}✓ Git hooks installed successfully!${NC}"
echo ""
echo "The pre-commit hook will now run 'cargo make ci-verify' before each commit."
echo "This ensures your code passes all CI checks before being committed."
echo ""
echo "To disable temporarily, use: git commit --no-verify"
echo "To disable permanently, run: git config --unset core.hooksPath"