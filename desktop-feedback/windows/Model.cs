using System;
using System.Globalization;
using System.Collections.Generic;

namespace DesktopFeedback {
    internal sealed class FeedbackState {
        internal Snapshot Snapshot { get; private set; }
        internal Session StopRequested { get; private set; }
        internal bool CanStop { get { return Snapshot != null && Snapshot.Session != null && Snapshot.Phase != "closed" && !Snapshot.Session.Same(StopRequested); } }
        internal bool Apply(Snapshot next) {
            if (Snapshot != null && next.Sequence <= Snapshot.Sequence) return false;
            if (Snapshot == null || next.Session == null || !next.Session.Same(Snapshot.Session)) StopRequested = null;
            Snapshot = next; return true;
        }
        internal Session RequestStop() {
            if (!CanStop) return null;
            StopRequested = Snapshot.Session; return StopRequested;
        }
        internal string Status {
            get {
                Snapshot s = Snapshot;
                if (s == null) return "Waiting for Host";
                if (s.Cleanup == "failed") return "Cleanup failed · inputs may remain held";
                if (s.Cleanup == "unknown") return "Cleanup unknown · not confirmed";
                if (s.Cleanup == "pending") return "Stopping · cleanup pending";
                if (s.Phase == "closed") return s.Cleanup == "released" ? "Session closed · inputs released" : "Session closed · no cleanup required";
                if (StopRequested != null) return "Stop requested · awaiting Host";
                if (s.Phase == "faulted") return s.Session == null ? "Host fault · no control session" : "Host fault · stop available";
                if (s.Session == null) return "Waiting for control session";
                switch (s.Phase) {
                    case "starting": return "Connecting to Host";
                    case "idle": return "Control held · waiting for Agent";
                    case "observing": return "Observing · no input action";
                    case "executing": return "Executing confirmed actions";
                    case "paused": return "Control paused";
                    case "stopping": return "Stopping · awaiting cleanup";
                    default: return "State unknown";
                }
            }
        }
    }
    internal static class Geometry {
        internal static bool Contains(Surface s, double x, double y) { return x >= s.X && y >= s.Y && x < s.X + s.Width && y < s.Y + s.Height; }
        internal static double AppKitY(double y, Surface s, double frameTop) { return frameTop - (y - s.Y); }
    }
    // Value-only boundary: supplied facts only; no desktop/WinForms/native APIs.
    internal sealed class ScreenFacts {
        internal readonly string DeviceName;
        internal readonly System.Drawing.Rectangle Bounds, WorkingArea;
        internal ScreenFacts(string deviceName, System.Drawing.Rectangle bounds, System.Drawing.Rectangle workingArea) {
            DeviceName = deviceName; Bounds = bounds; WorkingArea = workingArea;
        }
        internal bool IsValid {
            get {
                return !String.IsNullOrWhiteSpace(DeviceName) && ValidRectangle(Bounds) &&
                    ValidRectangle(WorkingArea) && Bounds.Contains(WorkingArea);
            }
        }
        internal static bool ValidRectangle(System.Drawing.Rectangle rectangle) {
            // Rectangle.Right/Bottom use unchecked int addition; validate before use.
            return rectangle.Width > 0 && rectangle.Height > 0 &&
                (long)rectangle.X + rectangle.Width <= Int32.MaxValue &&
                (long)rectangle.Y + rectangle.Height <= Int32.MaxValue;
        }
        internal bool Contains(double x, double y) {
            return x >= Bounds.Left && y >= Bounds.Top && x < Bounds.Right && y < Bounds.Bottom;
        }
    }
    internal sealed class ScreenResolution {
        internal readonly ScreenFacts Anchor, PointerScreen;
        internal readonly System.Drawing.Point RingCenter;
        internal bool CanShowPointer { get { return PointerScreen != null; } }
        internal ScreenResolution(ScreenFacts anchor, ScreenFacts pointerScreen, Pointer pointer) {
            Anchor = anchor; PointerScreen = pointerScreen;
            if (pointerScreen != null) {
                System.Drawing.Rectangle b = pointerScreen.Bounds;
                // Rounding a fractional coordinate at a screen edge must not move
                // the center into a bbox gap or an unselected neighboring screen.
                RingCenter = new System.Drawing.Point(
                    (int)Math.Max(b.Left, Math.Min(b.Right - 1, Math.Round(pointer.X))),
                    (int)Math.Max(b.Top, Math.Min(b.Bottom - 1, Math.Round(pointer.Y))));
            }
        }
    }
    internal static class ScreenResolver {
        internal static bool Finite(double value) { return !Double.IsNaN(value) && !Double.IsInfinity(value); }
        internal static ScreenResolution Resolve(Surface surface, Pointer pointer, IList<ScreenFacts> screens) {
            if (surface == null || String.IsNullOrEmpty(surface.Id) || screens == null || screens.Count == 0 ||
                !Finite(surface.X) || !Finite(surface.Y) || !Finite(surface.Width) || !Finite(surface.Height) ||
                surface.Width <= 0 || surface.Height <= 0 || !Finite(surface.X + surface.Width) || !Finite(surface.Y + surface.Height)) return null;
            foreach (ScreenFacts screen in screens) if (screen == null || !screen.IsValid) return null;
            bool pointerInSurface = pointer != null && Finite(pointer.X) && Finite(pointer.Y) && Geometry.Contains(surface, pointer.X, pointer.Y);
            if (surface.Id == "desktop") return ResolveDesktop(surface, pointer, pointerInSurface, screens);
            // Legacy single-display matching retains ordered first-match + <0.5
            // tolerance, including mirrors. This path can NEVER accept a union.
            foreach (ScreenFacts candidate in screens) {
                System.Drawing.Rectangle b = candidate.Bounds;
                if (Math.Abs(surface.X - b.X) < 0.5 && Math.Abs(surface.Y - b.Y) < 0.5 &&
                    Math.Abs(surface.Width - b.Width) < 0.5 && Math.Abs(surface.Height - b.Height) < 0.5) {
                    return new ScreenResolution(candidate,
                        pointerInSurface && candidate.Contains(pointer.X, pointer.Y) ? candidate : null, pointer);
                }
            }
            return null;
        }
        static ScreenResolution ResolveDesktop(Surface surface, Pointer pointer, bool pointerInSurface, IList<ScreenFacts> screens) {
            long left = screens[0].Bounds.Left, top = screens[0].Bounds.Top;
            long right = screens[0].Bounds.Right, bottom = screens[0].Bounds.Bottom;
            HashSet<string> names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            ScreenFacts pointerScreen = null;
            for (int i = 0; i < screens.Count; i++) {
                ScreenFacts screen = screens[i]; System.Drawing.Rectangle b = screen.Bounds;
                if (!names.Add(screen.DeviceName)) return null;
                for (int j = 0; j < i; j++) {
                    System.Drawing.Rectangle other = screens[j].Bounds;
                    // Positive-area overlap (including mirrors) is ambiguous.
                    // Touching edges/corners and disjoint screens with gaps are valid.
                    if (b.Left < other.Right && other.Left < b.Right && b.Top < other.Bottom && other.Top < b.Bottom) return null;
                }
                left = Math.Min(left, b.Left); top = Math.Min(top, b.Top);
                right = Math.Max(right, b.Right); bottom = Math.Max(bottom, b.Bottom);
                if (pointerInSurface && screen.Contains(pointer.X, pointer.Y)) pointerScreen = screen;
            }
            long width = right - left, height = bottom - top;
            if (width > Int32.MaxValue || height > Int32.MaxValue || surface.X != left || surface.Y != top ||
                surface.Width != width || surface.Height != height) return null;
            // Stable first selected screen, independent of pointer/gap. Do not use
            // desktop bbox origin as a workarea or move Stop whenever pointer moves.
            return new ScreenResolution(screens[0], pointerScreen, pointer);
        }
    }
    internal sealed class OverlayLayout {
        internal readonly System.Drawing.Rectangle Bar, Stop;
        internal readonly float Scale;
        OverlayLayout(System.Drawing.Rectangle bar, System.Drawing.Rectangle stop, float scale) {
            Bar = bar; Stop = stop; Scale = scale;
        }
        internal static OverlayLayout Calculate(ScreenFacts screen, double dpi) {
            if (screen == null || !screen.IsValid) throw new WireFailure("surface_geometry_unavailable");
            if (!ScreenResolver.Finite(dpi) || dpi < 1) throw new WireFailure("window_dpi_failed");
            System.Drawing.Rectangle area = screen.WorkingArea;
            double scale = dpi / 96;
            // Keep readable status + full-sized Stop. Never silently shrink Stop,
            // stretch past WorkingArea, or claim visibility when space is insufficient.
            double margin = Math.Ceiling(16 * scale), height = Math.Ceiling(48 * scale);
            double button = Math.Ceiling(88 * scale), gap = Math.Ceiling(8 * scale);
            double minimumBar = Math.Ceiling(80 * scale);
            double width = Math.Min(Math.Ceiling(448 * scale), area.Width - 2 * margin);
            if (width < minimumBar + gap + button || area.Height < height + 2 * margin)
                throw new WireFailure("surface_workarea_too_small");
            // Every dimension now fits the validated int workarea; only now cast.
            int x = area.Left + (int)margin, y = area.Top + (int)margin;
            System.Drawing.Rectangle bar = new System.Drawing.Rectangle(x, y, (int)(width - button - gap), (int)height);
            System.Drawing.Rectangle stop = new System.Drawing.Rectangle(bar.Right + (int)gap, y, (int)button, (int)height);
            if (!area.Contains(bar) || !area.Contains(stop)) throw new WireFailure("surface_workarea_too_small");
            return new OverlayLayout(bar, stop, (float)scale);
        }
    }
    internal sealed class Style {
        internal readonly int Rgb;
        internal readonly string Label;
        Style(int rgb, string label) { Rgb = rgb; Label = label; }
        internal static Style Parse(string[] args) {
            int rgb = 0x2563EB; string label = "AI control"; HashSet<string> seen = new HashSet<string>();
            for (int i = 0; i < args.Length; i += 2) {
                string key = args[i];
                if ((key != "--accent" && key != "--label") || !seen.Add(key) || i + 1 >= args.Length) throw new WireFailure("invalid_options");
                string value = args[i + 1];
                if (key == "--accent") {
                    if (value.Length != 7 || value[0] != '#' || !Int32.TryParse(value.Substring(1), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out rgb)) throw new WireFailure("invalid_accent");
                    foreach (char c in value.Substring(1)) if (!(c >= '0' && c <= '9') && !(c >= 'A' && c <= 'F') && !(c >= 'a' && c <= 'f')) throw new WireFailure("invalid_accent");
                } else {
                    if (String.IsNullOrWhiteSpace(value) || value.Length > 48) throw new WireFailure("invalid_label");
                    for (int j = 0; j < value.Length; j++) {
                        UnicodeCategory category = CharUnicodeInfo.GetUnicodeCategory(value, j);
                        if (Char.IsControl(value[j]) || category == UnicodeCategory.Format || category == UnicodeCategory.LineSeparator || category == UnicodeCategory.ParagraphSeparator) throw new WireFailure("invalid_label");
                        if (Char.IsHighSurrogate(value[j])) { if (j + 1 >= value.Length || !Char.IsLowSurrogate(value[++j])) throw new WireFailure("invalid_label"); }
                        else if (Char.IsLowSurrogate(value[j])) throw new WireFailure("invalid_label");
                    }
                    label = value;
                }
            }
            return new Style(rgb, label);
        }
    }
}
