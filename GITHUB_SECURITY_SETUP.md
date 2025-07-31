# GitHub Security Configuration Guide

This guide walks through setting up comprehensive security measures for the mqtt-lib repository.

## 1. GPG Commit Signing Setup (Local)

### Generate GPG Key
```bash
# Generate a new GPG key
gpg --full-generate-key

# Use these settings:
# - Key type: RSA and RSA (default)
# - Key size: 4096 bits
# - Expiration: 2 years (recommended)
# - Real name: Your GitHub name
# - Email: Your GitHub email address
```

### Configure Git
```bash
# List your GPG keys
gpg --list-secret-keys --keyid-format=long

# Configure Git to use your GPG key (replace YOUR_KEY_ID)
git config --global user.signingkey YOUR_KEY_ID
git config --global commit.gpgsign true
git config --global tag.gpgSign true

# Export your public key for GitHub
gpg --armor --export YOUR_KEY_ID
```

### Add GPG Key to GitHub
1. Go to GitHub Settings → SSH and GPG keys
2. Click "New GPG key"
3. Paste your public key (from the export command above)
4. Click "Add GPG key"

## 2. Branch Protection Rules Setup

Navigate to your repository on GitHub: `Settings → Branches → Add rule`

### Main Branch Protection

**Branch name pattern:** `main`

**Protection rules to enable:**

✅ **Restrict pushes that create files**
- Prevent accidental file creation on main

✅ **Require a pull request before merging**
- Require at least 1 approving review
- Dismiss stale reviews when new commits are pushed
- Require review from code owners (if CODEOWNERS file exists)

✅ **Require status checks to pass before merging**
- Require branches to be up to date before merging
- Add these required status checks:
  - `Rustfmt`
  - `Clippy` 
  - `Test Suite`
  - `MSRV (1.82)`
  - `Security Audit`
  - `Documentation`
  - `Integration Tests`

✅ **Require signed commits**
- All commits must be signed with GPG

✅ **Require linear history**
- Prevent merge commits, require squash or rebase

✅ **Include administrators**
- Apply rules to repository administrators

✅ **Restrict force pushes**
- Prevent force pushes to main

✅ **Allow deletions**
- Uncheck this - prevent branch deletion

### Develop Branch Protection

**Branch name pattern:** `develop`

**Protection rules to enable:**

✅ **Require status checks to pass before merging**
- Require branches to be up to date before merging
- Same status checks as main branch

✅ **Require signed commits**
- All commits must be signed with GPG

✅ **Restrict force pushes**
- Prevent force pushes to develop

## 3. Repository Security Settings

Navigate to `Settings → Security`

### Code security and analysis

✅ **Dependency graph**
- Automatically enabled for public repos

✅ **Dependabot alerts**
- Get notified about vulnerable dependencies

✅ **Dependabot security updates**
- Automatically create PRs to fix vulnerabilities

✅ **Dependabot version updates**
- Create `dependabot.yml` config file:

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "09:00"
    reviewers:
      - "fabriciobracht"
    assignees:
      - "fabriciobracht"
    commit-message:
      prefix: "deps"
      include: "scope"
```

✅ **Code scanning**
- Enable CodeQL analysis
- Use default query suite
- Run on push and pull request

✅ **Secret scanning**
- Automatically enabled for public repos
- Enable push protection (prevents committing secrets)

## 4. Webhook Security (Optional)

If you use webhooks:
- Always use HTTPS endpoints
- Verify webhook signatures
- Use secret tokens
- Implement proper authentication

## 5. Third-party Access Audit

Regularly review:
- Installed GitHub Apps
- OAuth applications
- Deploy keys
- Personal access tokens
- Collaborator permissions

## 6. Supply Chain Security Verification

Our workflows now include:
- Dependency vulnerability scanning
- License compliance checking
- Commit signature verification
- Secret detection
- SBOM generation
- Reproducible build verification

## 7. Incident Response

In case of security incident:
1. Revoke potentially compromised credentials
2. Force-rotate any secrets
3. Review audit logs
4. Update dependencies
5. Re-sign and re-deploy if needed

## 8. Regular Security Maintenance

### Weekly
- Review Dependabot PRs
- Check security advisories

### Monthly  
- Audit repository access
- Review workflow runs
- Update security tools

### Quarterly
- Rotate GPG keys if needed
- Security policy review
- Penetration testing (if applicable)

## Implementation Checklist

- [ ] Generate and configure GPG key locally
- [ ] Add GPG public key to GitHub account
- [ ] Configure main branch protection rules
- [ ] Configure develop branch protection rules
- [ ] Enable all repository security features
- [ ] Create dependabot.yml configuration
- [ ] Test commit signing works
- [ ] Verify branch protection blocks unsigned commits
- [ ] Test that all CI checks are required
- [ ] Document any exceptions or special procedures

## Security Contact

For security issues, contact: fabricio@bracht.dev

**Note**: Never commit security credentials, private keys, or sensitive configuration to the repository.