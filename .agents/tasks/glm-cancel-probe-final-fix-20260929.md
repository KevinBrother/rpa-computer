Implement NOW, don't spend turns rediscovering source. ONLY edit .agents/runs/glm-cancel-live-probe-20260929.py. Coordinator verified runtime: key_hold plan is press/sleep/release, events_total=2 excluding sleep, completed press index0. Post-cancel guard_active returns error.code cancelled. Close returns state closed cleanup released. Existing file is 219 lines. Ignore line cap.
Use 1 Read current file, 1 Bash Python exact replacements or several Edit, then py_compile + synthetic sanitize test. No live probe. No extra source browsing.
Required fixes:
- sanitize recursively traverse dict/list, replace data ONLY when dict type=image; nested result.content image base64 must not survive.
- save initial PNG metadata into variable; assert image_saved and (width,height)==(960,540).
- replace generic cancellation check with resp.result.isError true and parsed meta: cancelled is True, error.code=='cancelled', input_outcome=='partial', cleanup_outcome=='released', events_completed==[0], events_total==2. Require response elapsed<=1.0 seconds (wait stays3).
- replace post-cancel move substring check (request_id itself has cancel!) with parsed metadata.error.code=='cancelled' and no input_outcome in ('dispatched','partial','unknown'), no nonempty events_completed. Code itself is pre-dispatch evidence if input_outcome absent.
- get_step request_id==step_req_id, cancelled True, cleanup_outcome=='released' exactly.
- close result metadata state=='closed' and cleanup_outcome=='released', isError false.
- main catch Exceptions from run and save nonzero summary. Summary must not show secrets/images. Remove unused CANCEL_HINTS.
Report changes and tests. This is a bounded verification helper, NOT a product feature and not model-driven acceptance.
