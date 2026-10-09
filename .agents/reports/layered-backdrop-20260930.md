# Layered backdrop diagnostic — 2026-09-30 (bounded helper task)

Scope: read-only probe + own-scope helper fix/compile only. No fixture/src changes, no GUI input, no commit/push, did not launch the helper (coordinator does).

## Root cause

**The fixture is NOT mis-positioned — it is occluded by the backdrop helper's own window.**

Evidence from read-only probe `.agents/runs/layered-backdrop-20260930/probe-1.json`
(script: `layered-backdrop-probe-20260930.py --pids 73685,73709`):

- CG displays: main id=2 bounds [0,0,1920,1080] (active); secondary id=1 [1920,0,1920,1080] (active).
- NSScreen (read-only Swift query, no NSApp created): screen0 = display 2 frame [0,0,1920,1080] `NSScreen.main`; screen1 = display 1 frame [1920,0,1920,1080]. **CG and NSScreen agree; the old x2430 offset problem is gone.**
- Fixture (pid 73685) main window #21667: bounds **[510,144,900,792]** — exactly centered on the main display (center 960,540 ✓), `on_screen=true`, layer 0, alpha 1.
- Backdrop (pid 73709) main window #21677: bounds [0,0,1920,1080], `on_screen=true`.

**Stacking (CGWindowList order, index 0 = frontmost):**

| window | owner | stacking index | verdict |
|---|---|---|---|
| #21677 backdrop 1920×1080 @0,0 | 73709 | **6** | in FRONT |
| #21667 fixture 900×792 @510,144 | 73685 | **10** | BEHIND backdrop |

The fixture is on-screen, on the correct (main) display, at the correct size — but buried under the backdrop. The existing helper's one-shot `fixtureApp.activate()` + `activate(options:[.activateAllWindows])` either never won the ordering race or was later re-covered by the backdrop windows (`orderFrontRegardless()` / subsequent reorders). GLM's "fixture visible" claim from the menu bar alone is not valid — the menu-bar strips (multiple 1920×30 windows of pid 73685) are separately visible while the content window is covered.

Side observation: pid 73709 also owns window #21678 at **[3840,0,1920,1080]** — entirely off both displays (displays span x 0–3840). A stray backdrop window, likely from a different screen arrangement at helper launch. Harmless but wasteful; new helper filters it.

## Fix implemented (my scope only, compiled, not launched)

Copy of `native-gui-helpers-20260929/macos-backdrop.swift` → `.agents/runs/layered-backdrop-20260930/macos-backdrop.swift`, changes:

1. Activation order: helper activates **itself** first (`NSApp.activate(ignoringOtherApps: true)`) so it owns the frontmost slot, then raises the fixture (`activate()` + `activate(options:[.activateAllWindows])`). Only the two owned apps are ever activated.
2. New 3s repeating timer re-asserts fixture activation for the helper's lifetime, so the backdrop can never permanently re-cover the fixture. No level elevation, no geometry change, no other apps touched.
3. Backdrop windows are created only for NSScreens intersecting active CG display bounds (eliminates the stray off-screen window).

Kept unchanged: PID+exe identity validation before any UI, 20-min hard cap, fixture watchdog (5s), SIGTERM handler, `BACKDROP_PID=` stdout identity line, accessory policy, no AppleScript/AX/keys/mouse.

## Commands / outputs

```
python3 .agents/runs/layered-backdrop-20260930/layered-backdrop-probe-20260930.py \
  --pids 73685,73709 --output .agents/runs/layered-backdrop-20260930/probe-1.json
# → probe-1.json (full JSON above-referenced), no errors, window_list_total=515

cd .agents/runs/layered-backdrop-20260930 && swiftc -O -o macos-backdrop macos-backdrop.swift
# → compiled OK (92176 bytes). ./macos-backdrop with no args → usage + exit 2 (validation path intact).
```

## Conclusion

- Coordinate fix in fixture: **correct, no counter-example found** — nothing to hand the fixture writer.
- Problem is purely backdrop occlusion → **helper change in my copy is the right and sufficient fix**. No main-thread/coordinate changes needed anywhere.
- Coordinator: launch the new helper from `.agents/runs/layered-backdrop-20260930/macos-backdrop` with `--fixture-pid 73685 --fixture-exe /private/tmp/computer-layered-mac-fixture-20260930-c/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance`, then re-run the probe to confirm fixture stacking index < backdrop index before trusting any screenshot.

## 11:30 CST 后续证据更正

旧helper反复activation版本仍不作为最终使用方案；没有接受它的“已足够修复”结论。最新-d fixture自有--coordinator-raise日志READY与pid/exe核验后发SIGUSR1，oracle事件已记录。虽然03:19:02Z瞬时probe显示owned fixture stacking43而bg0，稍后MCP实际截图 `.agents/runs/layered-mac-baseline-d-preflight-frame-20260930.png` 已看见窗口；说明瞬时probe不能单独代替屏幕验收。新观察仍未达到测试门禁，因为顶部多个label不显示，未让模型盲测。该问题另外单独定位，不归因为GLM识别错误。所有旧空白/菜单误报/失败probe保留。
