using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace DesktopFeedback {
    internal class PassiveForm : Form {
        readonly bool interactive;
        internal Action Stop;
        internal bool HostShutdown;
        internal PassiveForm(bool interactive) {
            this.interactive = interactive;
            FormBorderStyle = FormBorderStyle.None; ShowInTaskbar = false; TopMost = true;
            StartPosition = FormStartPosition.Manual; AutoScaleMode = AutoScaleMode.None;
            DoubleBuffered = true;
            // Both decorations are actual layered HWNDs; TRANSPARENT only on them.
            // The Stop HWND deliberately does NOT have WS_EX_TRANSPARENT.
            Opacity = 0.97;
        }
        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams {
            get {
                CreateParams p = base.CreateParams;
                p.ExStyle |= 0x08000000 | 0x00000080 | 0x00080000; // NOACTIVATE, TOOLWINDOW, LAYERED
                if (!interactive) p.ExStyle |= 0x00000020; // TRANSPARENT (layered click-through)
                return p;
            }
        }
        protected override void WndProc(ref Message message) {
            if (message.Msg == 0x0021) { message.Result = new IntPtr(3); return; } // MA_NOACTIVATE, not AND_EAT
            base.WndProc(ref message);
        }
        protected override void OnFormClosing(FormClosingEventArgs e) {
            if (!HostShutdown) { e.Cancel = true; if (Stop != null) Stop(); }
            base.OnFormClosing(e);
        }
    }
    internal sealed class StatusForm : PassiveForm {
        readonly Style style;
        readonly Color accent;
        internal string Status = "Waiting for Host";
        float scale = 1;
        internal StatusForm(Style style) : base(false) {
            this.style = style; accent = Color.FromArgb((style.Rgb >> 16) & 255, (style.Rgb >> 8) & 255, style.Rgb & 255);
            BackColor = Color.FromArgb(20, 20, 22); AccessibleName = style.Label;
        }
        internal void ScaleTo(float value) { scale = value; Invalidate(); }
        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e); Graphics g = e.Graphics; g.SmoothingMode = SmoothingMode.AntiAlias;
            using (Pen border = new Pen(Color.FromArgb(90, 90, 96))) g.DrawRectangle(border, 0, 0, ClientSize.Width - 1, ClientSize.Height - 1);
            using (Brush brush = new SolidBrush(accent)) g.FillEllipse(brush, 12 * scale, 20 * scale, 8 * scale, 8 * scale);
            using (Pen outline = new Pen(Color.White, scale)) g.DrawEllipse(outline, 12 * scale, 20 * scale, 8 * scale, 8 * scale);
            Rectangle name = new Rectangle((int)(30 * scale), (int)(6 * scale), Math.Max(1, ClientSize.Width - (int)(42 * scale)), (int)(17 * scale));
            Rectangle status = new Rectangle(name.X, (int)(26 * scale), name.Width, (int)(17 * scale));
            using (Font titleFont = new Font("Segoe UI", 12 * scale, FontStyle.Bold, GraphicsUnit.Pixel))
            using (Font stateFont = new Font("Segoe UI", 11 * scale, FontStyle.Regular, GraphicsUnit.Pixel)) {
                TextRenderer.DrawText(g, style.Label, titleFont, name, Color.White, TextFormatFlags.EndEllipsis | TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix);
                TextRenderer.DrawText(g, Status, stateFont, status, Color.FromArgb(219, 219, 225), TextFormatFlags.EndEllipsis | TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix);
            }
        }
    }
    internal sealed class StopForm : PassiveForm {
        internal readonly Button Button = new Button();
        readonly ToolTip tip = new ToolTip();
        float scale;
        Font ownedFont;
        internal StopForm() : base(true) {
            BackColor = Color.FromArgb(20, 20, 22);
            Button.Text = "Stop"; Button.Dock = DockStyle.Fill;
            Button.FlatStyle = FlatStyle.Flat; Button.FlatAppearance.BorderColor = Color.FromArgb(90, 90, 96);
            Button.FlatAppearance.BorderSize = 1; Button.BackColor = Color.FromArgb(245, 246, 250); Button.ForeColor = Color.FromArgb(25, 26, 31);
            Button.Enabled = false; Button.AccessibleName = "Stop AI control session";
            tip.SetToolTip(Button, "Request cancellation and input cleanup from Host");
            Button.Click += delegate { if (Stop != null) Stop(); };
            Controls.Add(Button);
        }
        internal void ScaleTo(float value) {
            if (scale == value) return; scale = value;
            Font old = ownedFont;
            ownedFont = new Font("Segoe UI", 14 * scale, FontStyle.Bold, GraphicsUnit.Pixel); Button.Font = ownedFont;
            if (old != null) old.Dispose();
        }
        protected override void Dispose(bool disposing) { if (disposing) { tip.Dispose(); if (ownedFont != null) ownedFont.Dispose(); } base.Dispose(disposing); }
    }
    internal sealed class RingForm : PassiveForm {
        readonly Color accent;
        internal string Kind = "move";
        internal RingForm(Style style) : base(false) {
            accent = Color.FromArgb((style.Rgb >> 16) & 255, (style.Rgb >> 8) & 255, style.Rgb & 255);
            BackColor = Color.Magenta; TransparencyKey = Color.Magenta; Opacity = 1;
        }
        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e); e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            float scale = ClientSize.Width / 44f;
            RectangleF rect = new RectangleF(5 * scale, 5 * scale, ClientSize.Width - 10 * scale, ClientSize.Height - 10 * scale);
            using (Pen white = new Pen(Color.White, (Kind == "click" ? 5 : 3) * scale)) e.Graphics.DrawEllipse(white, rect);
            rect.Inflate(-scale, -scale);
            using (Pen pen = new Pen(accent, (Kind == "click" ? 3 : 2) * scale)) e.Graphics.DrawEllipse(pen, rect);
        }
    }
    internal sealed class FeedbackWindows : IDisposable {
        internal readonly StatusForm Bar;
        internal readonly StopForm Stop;
        internal readonly RingForm Ring;
        internal Action RequestStop;
        Screen fallback;
        internal FeedbackWindows(Style style) {
            fallback = Screen.PrimaryScreen;
            if (fallback == null) throw new WireFailure("gui_display_unavailable");
            Bar = new StatusForm(style); Stop = new StopForm(); Ring = new RingForm(style);
            Bar.Stop = Stop.Stop = delegate { if (RequestStop != null) RequestStop(); };
            Place(Facts(fallback));
            // Allocate the ring's real top-level HWND before requesting affinity.
            IntPtr ringHandle = Ring.Handle;
            if (Bar.Handle == IntPtr.Zero || Stop.Handle == IntPtr.Zero || ringHandle == IntPtr.Zero || !Bar.Visible || !Stop.Visible) throw new WireFailure("gui_initialization_failed");
        }
        internal IntPtr[] Handles { get { return new [] {Bar.Handle, Stop.Handle, Ring.Handle}; } }
        static ScreenFacts Facts(Screen screen) {
            return new ScreenFacts(screen.DeviceName, screen.Bounds, screen.WorkingArea);
        }
        void HideFeedback() { Ring.Hide(); Bar.Hide(); Stop.Hide(); }
        void Place(ScreenFacts screen) {
            try {
                if (screen == null || !screen.IsValid) throw new WireFailure("surface_geometry_unavailable");
                // Probe DPI only on the selected real screen, not desktop bbox/gap.
                // Hide before a cross-monitor move so old dimensions cannot leave a
                // temporarily off-screen Stop; ShowWithoutActivation is preserved.
                if (!screen.Bounds.Contains(Bar.Location)) {
                    Bar.Hide(); Stop.Hide(); Bar.Location = screen.WorkingArea.Location;
                }
                uint dpi = Native.GetDpiForWindow(Bar.Handle);
                OverlayLayout layout = OverlayLayout.Calculate(screen, dpi);
                Bar.ScaleTo(layout.Scale); Stop.ScaleTo(layout.Scale);
                Bar.Bounds = layout.Bar; Stop.Bounds = layout.Stop;
                // Refuse OS-enforced minimum-size/DPI alterations rather than report
                // a reachable Stop using only the requested, not actual, rectangles.
                if (Bar.Bounds != layout.Bar || Stop.Bounds != layout.Stop ||
                    Native.GetDpiForWindow(Stop.Handle) != dpi) throw new WireFailure("surface_layout_unavailable");
                Bar.Show(); Stop.Show();
                if (!Bar.Visible || !Stop.Visible || !Stop.Button.Visible || Bar.Bounds != layout.Bar || Stop.Bounds != layout.Stop ||
                    Stop.Button.Width <= 0 || Stop.Button.Height <= 0 || !Stop.ClientRectangle.Contains(Stop.Button.Bounds))
                    throw new WireFailure("surface_layout_unavailable");
            } catch (WireFailure) { HideFeedback(); throw; }
        }
        internal string Render(FeedbackState state, bool showPointer) {
            Snapshot snapshot = state.Snapshot;
            Bar.Status = state.Status; Bar.AccessibleDescription = state.Status;
            Stop.Button.Enabled = state.CanStop;
            Screen screen = null;
            ScreenResolution resolution = null;
            if (snapshot != null && snapshot.Surface != null) {
                Screen[] candidates = Screen.AllScreens;
                ScreenFacts[] facts = new ScreenFacts[candidates.Length];
                for (int i = 0; i < candidates.Length; i++)
                    facts[i] = Facts(candidates[i]);
                resolution = ScreenResolver.Resolve(snapshot.Surface, snapshot.Pointer, facts);
                if (resolution == null) { HideFeedback(); throw new WireFailure("surface_geometry_unavailable"); }
                for (int i = 0; i < facts.Length; i++)
                    if (facts[i] == resolution.Anchor) { screen = candidates[i]; break; }
                if (screen == null) { HideFeedback(); throw new WireFailure("surface_geometry_unavailable"); }
            }
            if (screen != null) fallback = screen;
            else {
                bool stillPresent = false;
                foreach (Screen candidate in Screen.AllScreens) if (candidate.DeviceName == fallback.DeviceName) { fallback = candidate; stillPresent = true; break; }
                if (!stillPresent) fallback = Screen.PrimaryScreen;
                if (fallback == null) throw new WireFailure("gui_display_unavailable");
            }
            Place(resolution == null ? Facts(fallback) : resolution.Anchor);
            if (screen != null && showPointer && snapshot.Pointer != null && snapshot.Session != null && snapshot.Phase != "closed" && snapshot.Phase != "faulted" && state.StopRequested == null && resolution.CanShowPointer) {
                Pointer p = snapshot.Pointer;
                // Native PMv2 virtual screen pixels; no screenshot scale is applied.
                Ring.Location = resolution.RingCenter;
                uint dpi = Native.GetDpiForWindow(Ring.Handle);
                if (dpi == 0) throw new WireFailure("window_dpi_failed");
                double scaledSize = Math.Ceiling(44d * dpi / 96d);
                if (scaledSize > Int32.MaxValue) throw new WireFailure("window_dpi_failed");
                int size = (int)scaledSize;
                long x = (long)resolution.RingCenter.X - size / 2, y = (long)resolution.RingCenter.Y - size / 2;
                if (x < Int32.MinValue || y < Int32.MinValue || x + size > Int32.MaxValue || y + size > Int32.MaxValue)
                    throw new WireFailure("surface_layout_unavailable");
                Ring.Bounds = new Rectangle((int)x, (int)y, size, size);
                Ring.Kind = p.Kind; Ring.Invalidate(); Ring.Show();
            } else Ring.Hide();
            Bar.Invalidate(); return null;
        }
        internal void ExpirePointer() { Ring.Hide(); }
        public void Dispose() {
            Bar.HostShutdown = Stop.HostShutdown = Ring.HostShutdown = true;
            Ring.Close(); Bar.Close(); Stop.Close(); Ring.Dispose(); Bar.Dispose(); Stop.Dispose();
        }
    }
}
