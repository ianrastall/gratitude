param(
    [string]$Data = (Join-Path $PSScriptRoot '../data'),
    [string]$Schema = (Join-Path $PSScriptRoot '../schemas/berthaml.xsd')
)
$ErrorActionPreference = 'Stop'
$schemaSet = [System.Xml.Schema.XmlSchemaSet]::new()
$schemaSet.XmlResolver = [System.Xml.XmlUrlResolver]::new()
[void]$schemaSet.Add('urn:berthaml:1', [IO.Path]::GetFullPath($Schema))
$schemaSet.Compile()
$settings = [System.Xml.XmlReaderSettings]::new()
$settings.Schemas = $schemaSet
$settings.ValidationType = [System.Xml.ValidationType]::Schema
$settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
$settings.XmlResolver = $null
$failures = [Collections.Generic.List[string]]::new()
$settings.add_ValidationEventHandler([System.Xml.Schema.ValidationEventHandler]{
    param($sender, $eventArgs)
    $failures.Add($eventArgs.Message)
})
$files = Get-ChildItem -LiteralPath ([IO.Path]::GetFullPath($Data)) -Filter '*.xml' -Recurse
foreach ($file in $files) {
    $reader = [System.Xml.XmlReader]::Create($file.FullName, $settings)
    try { while ($reader.Read()) {} }
    finally { $reader.Dispose() }
}
if ($failures.Count) {
    $failures | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    throw "$($failures.Count) .NET schema validation failures"
}
Write-Output "$($files.Count) XML files: .NET XSD validation passed"
