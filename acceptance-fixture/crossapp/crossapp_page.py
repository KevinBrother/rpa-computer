"""Pure offline page templating; no browser, listener, external resource or IO."""
import html
import json


def render_page(case, oracle, template, css, script):
    config = {k: case[k] for k in ("case_id", "run_id", "trial_id", "nonce")}
    config.update(expected_text=oracle["expected_form_text"], expected_choice=oracle["expected_choice"],
                  filename=oracle["event_export_path"].split("/")[-1])
    replacements = {
        "@@CSS@@": css,
        "@@SCRIPT@@": script,
        "@@CONFIG@@": json.dumps(config, ensure_ascii=True).replace("<", "\\u003c"),
        "@@CASE@@": html.escape(case["case_id"]),
        "@@NONCE@@": html.escape(case["nonce"]),
        "@@TRIAL@@": html.escape(case["trial_id"]),
        "@@RUN@@": html.escape(case["run_id"]),
        "@@ENTRY@@": html.escape(oracle["expected_form_text"]),
        "@@ROWS@@": "\n".join('<div class="course-row"><b>%02d</b><span>VERTICAL CHECKPOINT</span><code>%s</code></div>' % (n, html.escape(case["nonce"][:8])) for n in range(1, 19)),
    }
    for key, value in replacements.items():
        template = template.replace(key, value)
    return template
