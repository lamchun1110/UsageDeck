[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$archiveUrl = 'https://ssl.com/wp-content/uploads/2024/06/CodeSignTool-v1.3.0-windows.zip'
$expectedSha256 = 'E22094505DECBE622AFE5B0C27ABC618ED2BA179BD94F3450490352399D5EF2A'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Windows signing can only be set up on Windows.'
}

$requiredVariables = @(
  'ES_USERNAME',
  'ES_PASSWORD',
  'ES_CREDENTIAL_ID',
  'ES_TOTP_SECRET',
  'RUNNER_TEMP',
  'GITHUB_ENV',
  'GITHUB_PATH'
)

foreach ($name in $requiredVariables) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
    throw "Required environment variable '$name' is missing."
  }
}

$workingDirectory = Join-Path $env:RUNNER_TEMP "usagedeck-codesigntool-$([Guid]::NewGuid().ToString('N'))"
$archivePath = Join-Path $workingDirectory 'CodeSignTool.zip'
$toolDirectory = Join-Path $workingDirectory 'tool'

New-Item -ItemType Directory -Path $workingDirectory | Out-Null

try {
  Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing

  $actualSha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToUpperInvariant()
  if ($actualSha256 -ne $expectedSha256) {
    throw "CodeSignTool checksum mismatch. Expected $expectedSha256, received $actualSha256."
  }

  Expand-Archive -LiteralPath $archivePath -DestinationPath $toolDirectory

  $launchers = @(Get-ChildItem -LiteralPath $toolDirectory -Recurse -File -Filter 'CodeSignTool.bat')
  if ($launchers.Count -ne 1) {
    throw "Expected exactly one CodeSignTool.bat, found $($launchers.Count)."
  }

  $toolRoot = $launchers[0].Directory.FullName
  $javaPath = Join-Path $toolRoot 'jdk-11.0.2\bin\java.exe'
  $jars = @(Get-ChildItem -LiteralPath (Join-Path $toolRoot 'jar') -File -Filter 'code_sign_tool-*.jar')

  if (-not (Test-Path -LiteralPath $javaPath -PathType Leaf)) {
    throw 'CodeSignTool Java runtime is missing.'
  }

  if ($jars.Count -ne 1) {
    throw "Expected exactly one CodeSignTool JAR, found $($jars.Count)."
  }

  @(
    "USAGEDECK_CODESIGNTOOL_JAVA=$javaPath"
    "USAGEDECK_CODESIGNTOOL_JAR=$($jars[0].FullName)"
  ) | Out-File -LiteralPath $env:GITHUB_ENV -Encoding utf8 -Append
  $PSScriptRoot | Out-File -LiteralPath $env:GITHUB_PATH -Encoding utf8 -Append

  Write-Host 'Windows signing tools are ready.'
}
finally {
  if (Test-Path -LiteralPath $archivePath -PathType Leaf) {
    Remove-Item -LiteralPath $archivePath -Force
  }
}
