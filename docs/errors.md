# Error recovery and private diagnostics

Pike keeps failures attached to their originating stable session. A passive
banner reserves terminal layout space. It does not cover the prompt or change
the typing owner. Dismiss removes that banner; a later failure may show again.
Repeated identical failures coalesce, with at most 64 retained notices.

| Failure | Recovery |
| --- | --- |
| Shell launch | Retry the frozen/current configured profile, or explicitly use the platform default shell. A successful retry receives a fresh session ID. |
| Terminal I/O or event overload | Final output stays available. Retry the terminal once its existing backend acknowledges actual cleanup. Other terminals remain usable. |
| Native resize | Retry the current desired size once, or dismiss. Identical frames do not retry automatically. A recovered banner retains its space until dismissed. |
| Paste or clipboard write | Retry reads the clipboard or current selection again, using the current owner. No failed payload is stored or automatically replayed. Old controls and old acknowledgments cannot target a newer owner/operation. |
| Rejected keyboard/IME input | Type the input again after the program reads input. Rejected input is not saved. |
| Settings | Correct settings.toml and retry loading it. The last valid settings and existing sessions remain active. |
| Command refusal | Follow the bounded explanation, then issue the command again. |
| Link opener | Check the default application, then open the link again. |
| Renderer/window startup | If a window cannot exist, Pike reports a fixed safe message and exits unsuccessfully. Check display/graphics availability and relaunch. No automatic renderer fallback occurs. |
| Cleanup timeout | The existing policy offers Wait by default and explicit Force close. Force exits Pike before cleanup is acknowledged. If rendering has stopped, Windows shows a fixed native choice through the same captured timeout token. |

No native dialog or renderer can be guaranteed if the operating system cannot
create windows. Failure to present the emergency choice produces one fixed safe
report; Pike keeps cleanup pending and never selects Force automatically.

Diagnostics are **off by default**. Start Pike with `--diagnostics` to enable
the in-memory metadata collector for that process. It records typed category,
owner/session IDs, bounded counts and optional duration/size/error-code values.
It accepts no arbitrary context string. It never records terminal input/output,
clipboard contents, environment values, shell arguments, paths or titles.

At most 64 records are retained, for 15 minutes from the first occurrence.
Identical errors coalesce without extending that lifetime. Expiration is checked
on application events and explicit capture; idle applications start no timer or
worker to expire data. Expired records are removed on the next such event, so
physical memory can remain until then. Nothing is written to a log, network or
file by this feature. Copy diagnostics explicitly exports only this bounded
metadata to the existing clipboard; review where you paste it.

Clear diagnostics releases retained records immediately. Embedders can also
disable diagnostics explicitly; disabling and normal shutdown clear its
allocation. Force close relies on operating-system process cleanup.
