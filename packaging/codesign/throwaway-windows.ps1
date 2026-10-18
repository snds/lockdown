# Signs the code-signature test's binaries with throwaway certificates
# (docs/plan/29-release-polish.md §2), for ci.yml's `codesign-windows` job.
# Two self-signed code-signing certificates are made here and trusted on
# this runner only for the job (LocalMachine\Root; the runner is thrown
# away after it); neither is a real identity. Then:
#
#   app-test.exe        the test binary (plays the app), signed with A
#   peer-same.exe       the peer (plays the host), signed with A
#   peer-other.exe      the peer, signed with B
#   peer-unsigned.exe   the peer, unsigned
#
#   pwsh packaging/codesign/throwaway-windows.ps1 -Out <dir> -TestBin <exe> -PeerBin <exe>
#
# Appends the environment the test reads to $env:GITHUB_ENV when set, and
# prints it.
param(
  [Parameter(Mandatory)] [string] $Out,
  [Parameter(Mandatory)] [string] $TestBin,
  [Parameter(Mandatory)] [string] $PeerBin
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$certs = @{}
foreach ($who in "A", "B") {
  $cert = New-SelfSignedCertificate -Type CodeSigningCert `
    -Subject "CN=Lockdown throwaway test signer $who, O=Lockdown CI" `
    -CertStoreLocation Cert:\CurrentUser\My -KeyAlgorithm RSA -KeyLength 2048 `
    -HashAlgorithm SHA256 -NotAfter (Get-Date).AddDays(1)
  $cer = Join-Path $Out "$who.cer"
  Export-Certificate -Cert $cert -FilePath $cer | Out-Null
  Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
  $certs[$who] = $cert
  Write-Host "throwaway signer ${who}: $($cert.Thumbprint)"
}
function Sign([string] $From, [string] $To, $Cert) {
  Copy-Item $From $To
  $r = Set-AuthenticodeSignature -FilePath $To -Certificate $Cert -HashAlgorithm SHA256
  if ($r.Status -ne "Valid") { throw "signing ${To}: $($r.Status) $($r.StatusMessage)" }
  Write-Host "signed $To with $($Cert.Subject)"
}
Sign $TestBin (Join-Path $Out "app-test.exe") $certs.A
Sign $PeerBin (Join-Path $Out "peer-same.exe") $certs.A
Sign $PeerBin (Join-Path $Out "peer-other.exe") $certs.B
Copy-Item $PeerBin (Join-Path $Out "peer-unsigned.exe")
$lines = @(
  "LOCKDOWN_CODESIGN_APP_TEST=$(Join-Path $Out 'app-test.exe')",
  "LOCKDOWN_CODESIGN_PEER_SAME=$(Join-Path $Out 'peer-same.exe')",
  "LOCKDOWN_CODESIGN_PEER_OTHER=$(Join-Path $Out 'peer-other.exe')",
  "LOCKDOWN_CODESIGN_PEER_UNSIGNED=$(Join-Path $Out 'peer-unsigned.exe')"
)
$lines | ForEach-Object { Write-Host $_ }
if ($env:GITHUB_ENV) { $lines | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8 }
