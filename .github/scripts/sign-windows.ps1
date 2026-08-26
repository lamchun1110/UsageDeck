[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateNotNullOrEmpty()]
  [string] $FilePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Windows signing can only run on Windows.'
}

$requiredVariables = @(
  'USAGEDECK_CODESIGNTOOL_JAVA',
  'USAGEDECK_CODESIGNTOOL_JAR',
  'ES_USERNAME',
  'ES_PASSWORD',
  'ES_CREDENTIAL_ID',
  'ES_TOTP_SECRET',
  'USAGEDECK_EXPECTED_WINDOWS_SIGNER_SUBJECT'
)

foreach ($name in $requiredVariables) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
    throw "Required environment variable '$name' is missing."
  }
}

$javaPath = $env:USAGEDECK_CODESIGNTOOL_JAVA
$jarPath = $env:USAGEDECK_CODESIGNTOOL_JAR
if (-not (Test-Path -LiteralPath $javaPath -PathType Leaf)) {
  throw 'CodeSignTool Java runtime does not exist.'
}

if (-not (Test-Path -LiteralPath $jarPath -PathType Leaf)) {
  throw 'CodeSignTool JAR does not exist.'
}

$resolvedFile = Resolve-Path -LiteralPath $FilePath -ErrorAction Stop
$targetPath = $resolvedFile.ProviderPath
if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
  throw 'The signing target is not a file.'
}

$arguments = @(
  'sign',
  "-username=$env:ES_USERNAME",
  "-password=$env:ES_PASSWORD",
  "-credential_id=$env:ES_CREDENTIAL_ID",
  "-totp_secret=$env:ES_TOTP_SECRET",
  "-input_file_path=$targetPath",
  '-override=true'
)

$toolRoot = Split-Path -Parent (Split-Path -Parent $jarPath)
$exitCode = 1
Push-Location -LiteralPath $toolRoot
try {
  & $javaPath '-jar' $jarPath @arguments
  $exitCode = $LASTEXITCODE
}
finally {
  Pop-Location
}

if ($exitCode -ne 0) {
  throw "CodeSignTool failed with exit code $exitCode."
}

$signature = Get-AuthenticodeSignature -LiteralPath $targetPath
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
  throw "Authenticode verification failed with status '$($signature.Status)'."
}

if ($null -eq $signature.SignerCertificate) {
  throw 'Authenticode verification did not return a signer certificate.'
}

if ($signature.SignerCertificate.Subject -ne $env:USAGEDECK_EXPECTED_WINDOWS_SIGNER_SUBJECT) {
  throw "Authenticode signer subject does not match the configured release identity."
}

if ($null -eq $signature.TimeStamperCertificate) {
  throw 'Authenticode verification did not return a timestamp certificate.'
}

Write-Host "Signed and verified '$targetPath'."
