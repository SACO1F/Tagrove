param([string]$Mode, [string]$Engine, [string]$Kind, [string]$InputFile, [string]$OutputFile)
$ErrorActionPreference = 'Stop'
$names = @{ microsoft = @{word='Word.Application'; excel='Excel.Application'; powerpoint='PowerPoint.Application'}; wps = @{word='KWPS.Application'; excel='KET.Application'; powerpoint='KWPP.Application'} }
function Find-Registration([string]$Vendor,[string]$Type) {
  $progId=$names[$Vendor][$Type]
  foreach ($bits in @(64,32)) {
    $view=if($bits -eq 64){[Microsoft.Win32.RegistryView]::Registry64}else{[Microsoft.Win32.RegistryView]::Registry32}
    $registry=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::ClassesRoot,$view)
    try {
      $key=$registry.OpenSubKey("$progId\CLSID")
      if (!$key) { continue }
      try { $clsid=[string]$key.GetValue('') } finally { $key.Dispose() }
      $key=$registry.OpenSubKey("CLSID\$clsid\LocalServer32")
      if (!$key) { continue }
      try { $server=[string]$key.GetValue('') } finally { $key.Dispose() }
      $match=[regex]::Match($server,'^\s*(?:"([^"]+\.exe)"|(.+?\.exe))(?:\s|$)','IgnoreCase')
      if (!$match.Success) { continue }
      $executable=[Environment]::ExpandEnvironmentVariables(($match.Groups[1].Value+$match.Groups[2].Value))
      if (Test-Path -LiteralPath $executable -PathType Leaf) {
        return @{progId=$progId;bits=$bits;executable=$executable}
      }
    } finally { $registry.Dispose() }
  }
  return $null
}
if ($Mode -eq 'detect') {
  $items = @()
  foreach ($vendor in @('microsoft','wps')) {
    foreach ($type in @('word','excel','powerpoint')) {
      $registration=Find-Registration $vendor $type
      if ($registration) { $items += @{engine=$vendor;kind=$type;bits=$registration.bits;progId=$registration.progId} }
    }
  }
  @{engines=$items} | ConvertTo-Json -Compress -Depth 4
  exit
}
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class SQOfficePid { [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p); }'
$registration=Find-Registration $Engine $Kind
if (!$registration) { throw 'OFFICE_NOT_REGISTERED: No usable local COM server was registered for this component. Repair the Office installation or choose another engine.' }
if ($registration.bits -ne ([IntPtr]::Size*8)) { throw 'OFFICE_BITNESS: Automation host architecture does not match the registered Office component.' }
$expected=[IO.Path]::GetFileNameWithoutExtension($registration.executable)
$allowedExecutables=@([IO.Path]::GetFullPath($registration.executable))
if ($Engine -eq 'wps') {
  $component=@{word='wps.exe';excel='et.exe';powerpoint='wpp.exe'}[$Kind]
  $allowedExecutables+= [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetDirectoryName($registration.executable)) $component))
}
$processNames=@($allowedExecutables | ForEach-Object {[IO.Path]::GetFileNameWithoutExtension($_)} | Select-Object -Unique)
if (Get-Process -Name $processNames -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero }) { throw 'OFFICE_IN_USE: Save and exit the selected Office application first. A shared WPS process may host Word, Excel and PPT; existing documents will not be attached or closed.' }
$before = @(Get-Process | Select-Object -ExpandProperty Id)
$app=$null; $doc=$null; $owned=$false; $previousLinks=$null; $stage='create isolated automation instance'
try {
  $app = New-Object -ComObject $registration.progId
  # Word exposes HWND on its document window, rather than Application.
  if ($Kind -eq 'word') { $doc=$app.Documents.Add(); $hwnd=$doc.ActiveWindow.Hwnd }
  elseif ($Kind -eq 'powerpoint') { $doc=$app.Presentations.Add(); $hwnd=$app.HWND }
  else { $doc=$app.Workbooks.Add(); $hwnd=$app.Hwnd }
  [uint32]$officePid=0
  if ($hwnd) { [void][SQOfficePid]::GetWindowThreadProcessId([IntPtr]$hwnd,[ref]$officePid) }
  if (!$officePid -and $Engine -eq 'wps') {
    # Some WPP builds do not expose HWND. Accept only one newly created server
    # from the registered installation, never a pre-existing shared process.
    $created=@(Get-Process -Name $processNames -ErrorAction SilentlyContinue | Where-Object {
      $before -notcontains $_.Id -and $allowedExecutables -icontains [IO.Path]::GetFullPath($_.MainModule.FileName)
    })
    $componentName=@{word='wps';excel='et';powerpoint='wpp'}[$Kind]
    $componentCreated=@($created | Where-Object {$_.ProcessName -ieq $componentName})
    if ($componentCreated.Count -eq 1) { $officePid=[uint32]$componentCreated[0].Id }
    elseif ($created.Count -eq 1) { $officePid=[uint32]$created[0].Id }
    else { throw ('OFFICE_IN_USE: Cannot prove ownership of the new WPS instance. New registered processes: '+ (($created | ForEach-Object {$_.ProcessName}) -join ',')+'. Existing instances are left untouched.') }
  }
  if (!$officePid -or $before -contains [int]$officePid) { throw 'OFFICE_IN_USE: A running Office instance cannot be isolated. Close that application and retry.' }
  $proc = Get-Process -Id $officePid
  if ($allowedExecutables -inotcontains [IO.Path]::GetFullPath($proc.MainModule.FileName)) { throw 'OFFICE_ENGINE_MISMATCH: Automation instance executable does not match the selected registered component.' }
  $owned=$true
  if ($doc) { if($Kind -eq 'powerpoint'){$doc.Close()}else{$doc.Close(0)}; [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($doc); $doc=$null }
  # Required: do not open documents if macro protection cannot be enabled.
  $app.AutomationSecurity=3
  $stage='open source document read-only'
  if ($Kind -eq 'word') {
    $app.Visible=$false; $app.DisplayAlerts=0
    $previousLinks=$app.Options.UpdateLinksAtOpen; $app.Options.UpdateLinksAtOpen=$false
    $doc=$app.Documents.Open($InputFile,$false,$true,$false)
    $stage='export Word preview'
    if ([IO.Path]::GetExtension($OutputFile) -eq ".docx") { if($Engine -eq 'wps'){$doc.SaveAs($OutputFile,16)}else{$doc.SaveAs2($OutputFile,16)} } else { $doc.ExportAsFixedFormat($OutputFile,17) }
  } elseif ($Kind -eq 'excel') {
    $app.Visible=$false; $app.DisplayAlerts=$false; $app.AskToUpdateLinks=$false
    $doc=$app.Workbooks.Open($InputFile,0,$true)
    $stage='export Excel preview'
    if ([IO.Path]::GetExtension($OutputFile) -eq ".xlsx") { $doc.CheckCompatibility=$false; $doc.SaveAs($OutputFile,51) } else {
      # Excel gridlines are normally not printed to PDF. Change the isolated read-only workbook only.
      foreach ($sheet in $doc.Worksheets) {
        try { $sheet.PageSetup.PrintGridlines=$true } finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($sheet) }
      }
      $doc.ExportAsFixedFormat(0,$OutputFile)
    }
  } else {
    $app.DisplayAlerts=1
    $doc=$app.Presentations.Open($InputFile,-1,0,0)
    $stage='export PowerPoint preview'
    if ([IO.Path]::GetExtension($OutputFile) -eq ".pptx") { $doc.SaveAs($OutputFile,24) } else { $doc.SaveAs($OutputFile,32) }
  }
  if (!(Test-Path -LiteralPath $OutputFile)) { throw 'OFFICE_EXPORT_EMPTY: The component did not create an output file.' }
} catch {
  [Console]::Error.WriteLine(('OFFICE_STAGE: {0}/{1}; {2}; {3}' -f $Engine,$Kind,$stage,$_.Exception.Message))
  throw
} finally {
  if ($doc) {
    try { if ($Kind -eq 'word') { $doc.Close(0) } elseif ($Kind -eq 'excel') { $doc.Close($false) } else { $doc.Close() } }
    catch { [Console]::Error.WriteLine('OFFICE_CLEANUP: document close failed; original conversion error is retained.') }
    try { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($doc) } catch {}
  }
  if ($app) {
    if ($owned) {
      try {
        if ($Kind -eq 'word' -and $null -ne $previousLinks) { $app.Options.UpdateLinksAtOpen=$previousLinks }
        $count = if ($Kind -eq 'word') { $app.Documents.Count } elseif ($Kind -eq 'excel') { $app.Workbooks.Count } else { $app.Presentations.Count }
        if ($count -eq 0) { $app.Quit() }
      } catch { [Console]::Error.WriteLine('OFFICE_CLEANUP: instance cleanup failed; no existing user process was terminated.') }
    }
    try { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($app) } catch {}
  }
}
