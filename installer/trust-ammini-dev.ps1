# Trust the throwaway self-signed certificate from a DRY-RUN installer build.
#
# The staging workflow (.github/workflows/build-windows-installer.yml) generates a
# fresh self-signed certificate when no real code-signing certificate is configured
# and ships both the public cert (Ammini-dry-run.cer) and this script in the
# installer artifact. Run this once, then run the installer:
#
#   powershell -ExecutionPolicy Bypass -File .\trust-ammini-dev.ps1
#
# It imports the certificate into the CURRENT USER's stores, so no admin rights are
# needed. This is a testing convenience only: a self-signed certificate does NOT
# give SmartScreen reputation, and a fresh certificate is generated on every build
# unless a real one is supplied via the WINDOWS_CERT_PFX_BASE64 /
# WINDOWS_CERT_PASSWORD repository secrets. Do not trust this on machines you care
# about.
$ErrorActionPreference = "Stop"

$cer = Join-Path $PSScriptRoot "Ammini-dry-run.cer"
if (-not (Test-Path $cer)) {
    throw "Ammini-dry-run.cer not found next to this script. Did the build use a real certificate?"
}

# Root: makes the chain trusted. TrustedPublisher: suppresses the "Do you want to
# install this software?" publisher prompt for the installer itself.
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\CurrentUser\Root | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\CurrentUser\TrustedPublisher | Out-Null

Write-Host "Trusted the Ammini dry-run certificate. You can now run the installer."
