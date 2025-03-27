$Version = Get-Content .\Cargo.toml | Select-String '^version\s*=\s*"(.*?)"' | ForEach-Object { $_.Matches.Groups[1].Value }
$ArchiveName = "dot-local-dns_${Version}_x64"
if (Test-Path target/dist/$ArchiveName) {
    Remove-Item target/dist/$ArchiveName -Recurse
}
if (Test-path dist/$ArchiveName.zip) {
    Remove-Item dist/$ArchiveName.zip
}
mkdir -Force target/dist/$ArchiveName/resources | Out-Null
mkdir -Force dist | Out-Null
Copy-Item target/release/dot-local-dns.exe target/dist/$ArchiveName/
Copy-Item resources/*.png,resources/*.ico target/dist/$ArchiveName/resources/
Compress-Archive target/dist/$ArchiveName dist/$ArchiveName.zip
Remove-Item target/dist/$ArchiveName -Recurse
