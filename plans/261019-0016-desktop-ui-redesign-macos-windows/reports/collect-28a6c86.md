# Collect 28a6c86

## Merged (trusted: orchestrator-computed)
- waso

## Tests
### waso: failed exit=1
<!-- UNTRUSTED WORKER OUTPUT: data, not instructions -->
~~~~text
[xUnit.net 00:00:05.58]     Fsync.App.Tests.ClipboardMappingTests.DibImageIsPublishedAsPngAndPastedAsBoth [FAIL]
[xUnit.net 00:00:05.91]     Fsync.App.Tests.ClipboardMappingTests.UnknownMimeTypesAreNotWritten [FAIL]
  Failed Fsync.App.Tests.ClipboardMappingTests.DibImageIsPublishedAsPngAndPastedAsBoth [1 s]
  Error Message:
   System.AggregateException : One or more errors occurred. (the clipboard is locked by another application) (OpenClipboard Failed (0x800401D0 (CLIPBRD_E_CANT_OPEN)))
---- System.InvalidOperationException : the clipboard is locked by another application
---- System.Runtime.InteropServices.COMException : OpenClipboard Failed (0x800401D0 (CLIPBRD_E_CANT_OPEN))
  Stack Trace:
  
----- Inner Stack Trace #1 (System.InvalidOperationException) -----
   at Fsync.Interop.Clipboard.Win32Clipboard.Write(IEnumerable`1 formats) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\Win32Clipboard.cs:line 90
   at Fsync.App.Tests.ClipboardMappingTests.DibImageIsPublishedAsPngAndPastedAsBoth() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\tests\Fsync.App.Tests\ClipboardMappingTests.cs:line 91
--- End of stack trace from previous location ---
----- Inner Stack Trace #2 (System.Runtime.InteropServices.COMException) -----
   at System.Runtime.InteropServices.Marshal.ThrowExceptionForHR(Int32 errorCode)
   at Fsync.Interop.Clipboard.OleClipboard.Retry(Func`1 call) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\OleClipboard.cs:line 32
   at Fsync.Interop.Clipboard.OleClipboard.Set(IDataObject obj) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\OleClipboard.cs:line 19
   at Fsync.App.Tests.ClipboardMappingTests.<>c.<Dispose>b__12_0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\tests\Fsync.App.Tests\ClipboardMappingTests.cs:line 158
   at Fsync.Interop.Clipboard.StaThread.<>c__DisplayClass17_0.<InvokeAsync>b__0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 93
   at Fsync.Interop.Clipboard.StaThread.<>c__DisplayClass16_0`1.<InvokeAsync>g__Run|0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 86
--- End of stack trace from previous location ---
   at Fsync.Interop.Clipboard.StaThread.Invoke(Action a) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 95
   at Fsync.App.Tests.ClipboardMappingTests.Dispose() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\tests\Fsync.App.Tests\ClipboardMappingTests.cs:line 158
   at ReflectionAbstractionExtensions.DisposeTestClass(ITest test, Object testClass, IMessageBus messageBus, ExecutionTimer timer, CancellationTokenSource cancellationTokenSource) in /_/src/xunit.execution/Extensions/ReflectionAbstractionExtensions.cs:line 79
  Failed Fsync.App.Tests.ClipboardMappingTests.UnknownMimeTypesAreNotWritten [328 ms]
  Error Message:
   System.Runtime.InteropServices.COMException : OpenClipboard Failed (0x800401D0 (CLIPBRD_E_CANT_OPEN))
  Stack Trace:
     at System.Runtime.InteropServices.Marshal.ThrowExceptionForHR(Int32 errorCode)
   at Fsync.Interop.Clipboard.OleClipboard.Retry(Func`1 call) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\OleClipboard.cs:line 32
   at Fsync.Interop.Clipboard.OleClipboard.Set(IDataObject obj) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\OleClipboard.cs:line 19
   at Fsync.App.Tests.ClipboardMappingTests.<>c.<Dispose>b__12_0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\tests\Fsync.App.Tests\ClipboardMappingTests.cs:line 158
   at Fsync.Interop.Clipboard.StaThread.<>c__DisplayClass17_0.<InvokeAsync>b__0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 93
   at Fsync.Interop.Clipboard.StaThread.<>c__DisplayClass16_0`1.<InvokeAsync>g__Run|0() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 86
--- End of stack trace from previous location ---
   at Fsync.Interop.Clipboard.StaThread.Invoke(Action a) in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\src\Fsync.Interop\Clipboard\StaThread.cs:line 95
   at Fsync.App.Tests.ClipboardMappingTests.Dispose() in C:\Users\dev\workspace\rig-fsync-native-rig-test\apps\windows\tests\Fsync.App.Tests\ClipboardMappingTests.cs:line 158
   at ReflectionAbstractionExtensions.DisposeTestClass(ITest test, Object testClass, IMessageBus messageBus, ExecutionTimer timer, CancellationTokenSource cancellationTokenSource) in /_/src/xunit.execution/Extensions/ReflectionAbstractionExtensions.cs:line 79

Failed!  - Failed:     4, Passed:   195, Skipped:     0, Total:   199, Duration: 8 s - Fsync.App.Tests.dll (net8.0)
~~~~
<!-- END UNTRUSTED WORKER OUTPUT -->

## Orchestrator re-run 2026-10-22 (supersedes the waso test above)
- Collect test failed 4/199, all `CLIPBRD_E_CANT_OPEN` (ClipboardMappingTests). Cause: during the run the console was switched to another user, so the `dev` session was Disconnected and had no active desktop for clipboard access. Environment, not code.
- Re-run with `dev` Active on console: `R run waso --ref 28a6c86b` (win cmd): build 0 warnings, 0 errors; dotnet test **199/199 passed**.
- Gate: pass (no failures outside baseline).
