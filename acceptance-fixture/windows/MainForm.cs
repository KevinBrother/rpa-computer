//
//  Computer Use Acceptance Fixture (Windows, WinForms)
//  A small native visual target range for screenshot-only computer-use agents.
//  No network, no file access except the optional coordinator-owned evidence file.
//
//  Usage: acceptance-fixture.exe [--seed N] [--evidence-file PATH]
//

using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace AcceptanceFixture
{
    // MARK: - Deterministic RNG (SplitMix64)

    sealed class SplitMix64
    {
        private ulong _state;
        public SplitMix64(ulong seed) { _state = seed; }
        public ulong Next()
        {
            unchecked
            {
                _state += 0x9E3779B97F4A7C15UL;
                ulong z = _state;
                z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL;
                z = (z ^ (z >> 27)) * 0x94D049BB133111EBUL;
                return z ^ (z >> 31);
            }
        }
        public int Below(int n) { return (int)(Next() % (ulong)n); }
        public void Shuffle<T>(T[] arr)
        {
            for (int i = arr.Length - 1; i > 0; i--)
            {
                int j = Below(i + 1);
                T tmp = arr[i]; arr[i] = arr[j]; arr[j] = tmp;
            }
        }
    }

    // MARK: - Evidence logger (coordinator-only JSONL oracle)

    sealed class EvidenceLogger
    {
        private readonly string _path;
        private EvidenceLogger(string path) { _path = path; }

        public static EvidenceLogger Create(string path)
        {
            if (string.IsNullOrEmpty(path)) return null;
            if (!File.Exists(path)) File.WriteAllText(path, "");
            return new EvidenceLogger(path);
        }

        private static string Escape(string s)
        {
            var sb = new StringBuilder(s.Length + 8);
            foreach (char c in s)
            {
                if (c == '"') sb.Append("\\\"");
                else if (c == '\\') sb.Append("\\\\");
                else if (c < 0x20) sb.Append("\\u" + ((int)c).ToString("x4"));
                else sb.Append(c);
            }
            return sb.ToString();
        }

        private static string Pair(string k, object v)
        {
            if (v is bool) return "\"" + k + "\":" + (((bool)v) ? "true" : "false");
            if (v is int || v is long || v is ulong) return "\"" + k + "\":" + v.ToString();
            return "\"" + k + "\":\"" + Escape(Convert.ToString(v)) + "\"";
        }

        public void Log(string type, params object[] kvPairs)
        {
            var sb = new StringBuilder();
            sb.Append("{\"type\":\"").Append(Escape(type))
              .Append("\",\"ts\":\"").Append(DateTime.UtcNow.ToString("o")).Append("\"");
            for (int i = 0; i + 1 < kvPairs.Length; i += 2)
                sb.Append(',').Append(Pair(Convert.ToString(kvPairs[i]), kvPairs[i + 1]));
            sb.Append('}');
            File.AppendAllText(_path, sb.ToString() + "\r\n", Encoding.UTF8);
        }
    }

    // MARK: - Trial model

    enum ShapeKind { Circle, Square, Triangle, Diamond }

    sealed class Item
    {
        public ShapeKind Kind;
        public Color Color;
        public string ColorName;
        public bool IsTarget;
    }

    static class Palette
    {
        public static readonly Color Green = Color.FromArgb(0, 184, 31);
        public static readonly Color Red = Color.FromArgb(220, 41, 41);
        public static readonly Color Blue = Color.FromArgb(31, 89, 220);
        public static readonly Color Yellow = Color.FromArgb(242, 199, 13);
    }

    // MARK: - Shapes canvas

    sealed class ShapesPanel : Panel
    {
        public Item[] Items = new Item[0];
        public event Action<int> SlotClicked;

        public ShapesPanel() { DoubleBuffered = true; BackColor = Color.White; }

        private Rectangle RectForSlot(int slot)
        {
            const int size = 110;
            int cx = Width * (2 * slot + 1) / 8;
            return new Rectangle(cx - size / 2, Height / 2 - size / 2, size, size);
        }

        private static GraphicsPath PathFor(ShapeKind kind, Rectangle r)
        {
            var p = new GraphicsPath();
            switch (kind)
            {
                case ShapeKind.Circle: p.AddEllipse(r); break;
                case ShapeKind.Square: p.AddRectangle(r); break;
                case ShapeKind.Triangle:
                    p.AddPolygon(new Point[]
                    {
                        new Point(r.X + r.Width / 2, r.Top),
                        new Point(r.Right, r.Bottom),
                        new Point(r.Left, r.Bottom)
                    });
                    break;
                case ShapeKind.Diamond:
                    p.AddPolygon(new Point[]
                    {
                        new Point(r.X + r.Width / 2, r.Top),
                        new Point(r.Right, r.Y + r.Height / 2),
                        new Point(r.X + r.Width / 2, r.Bottom),
                        new Point(r.Left, r.Y + r.Height / 2)
                    });
                    break;
            }
            return p;
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            for (int slot = 0; slot < Items.Length; slot++)
            {
                Rectangle r = RectForSlot(slot);
                r.Inflate(-4, -4);
                using (var path = PathFor(Items[slot].Kind, r))
                using (var brush = new SolidBrush(Items[slot].Color))
                using (var pen = new Pen(Color.Black, 3f))
                { e.Graphics.FillPath(brush, path); e.Graphics.DrawPath(pen, path); }
            }
        }

        protected override void OnMouseClick(MouseEventArgs e)
        {
            base.OnMouseClick(e);
            for (int slot = 0; slot < Items.Length; slot++)
            {
                if (RectForSlot(slot).Contains(e.Location))
                {
                    var handler = SlotClicked;
                    if (handler != null) handler(slot);
                    return;
                }
            }
        }
    }

    // MARK: - Main form

    sealed class MainForm : Form
    {
        private const string SampleText = "computer-use 你好 10×20";
        private const string NonceChars = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        private const string WindowTitle = "Computer Use Acceptance";

        private readonly SplitMix64 _rng;
        private readonly ulong _seedUsed;
        private readonly EvidenceLogger _logger;

        private readonly Label _trialLabel;
        private readonly Label _nonceLabel;
        private readonly Label _statusLabel;
        private readonly Label _resultLabel;
        private readonly TextBox _textBox;
        private readonly ShapesPanel _shapes;

        private int _trialIndex;
        private Item[] _items = new Item[0];
        private int _targetSlot;
        private string _nonce = "";

        public MainForm(string evidencePath, ulong? seed)
        {
            _seedUsed = seed ?? (ulong)DateTime.UtcNow.Ticks;
            _rng = new SplitMix64(_seedUsed);
            _logger = EvidenceLogger.Create(evidencePath);

            // --- form ---
            Text = WindowTitle;
            StartPosition = FormStartPosition.CenterScreen;
            FormBorderStyle = FormBorderStyle.FixedSingle;
            MaximizeBox = false;
            ClientSize = new Size(884, 622); // 900x650 incl. borders
            BackColor = Color.White;

            // --- controls ---
            _trialLabel = new Label
            {
                Bounds = new Rectangle(20, 8, 300, 28),
                Font = new Font(FontFamily.GenericSansSerif, 16f, FontStyle.Bold),
                TextAlign = ContentAlignment.MiddleLeft
            };
            _nonceLabel = new Label
            {
                Bounds = new Rectangle(20, 42, 844, 46),
                Font = new Font("Consolas", 24f, FontStyle.Bold),
                TextAlign = ContentAlignment.MiddleCenter
            };
            var instr = new Label
            {
                Bounds = new Rectangle(20, 92, 844, 24),
                Font = new Font(FontFamily.GenericSansSerif, 13f),
                TextAlign = ContentAlignment.MiddleCenter,
                Text = "TARGET: the only GREEN CIRCLE — click it"
            };
            _statusLabel = new Label
            {
                Bounds = new Rectangle(20, 120, 844, 42),
                Font = new Font(FontFamily.GenericSansSerif, 22f, FontStyle.Bold),
                TextAlign = ContentAlignment.MiddleCenter,
                BackColor = Color.White
            };
            _shapes = new ShapesPanel { Bounds = new Rectangle(20, 168, 844, 230) };
            _shapes.SlotClicked += ShapeClicked;
            var sample = new Label
            {
                Bounds = new Rectangle(20, 402, 844, 24),
                Font = new Font(FontFamily.GenericSansSerif, 13f),
                Text = "Type exactly:  " + SampleText
            };
            _textBox = new TextBox
            {
                Bounds = new Rectangle(20, 430, 844, 110),
                Multiline = true,
                ScrollBars = ScrollBars.Vertical,
                Font = new Font(FontFamily.GenericSansSerif, 13f)
            };
            _resultLabel = new Label
            {
                Bounds = new Rectangle(20, 552, 330, 38),
                Font = new Font(FontFamily.GenericSansSerif, 11f, FontStyle.Bold),
                TextAlign = ContentAlignment.MiddleLeft
            };
            var checkBtn = new Button { Bounds = new Rectangle(360, 550, 160, 42), Text = "Check text" };
            var nextBtn = new Button { Bounds = new Rectangle(540, 550, 160, 42), Text = "Next trial" };
            var closeBtn = new Button { Bounds = new Rectangle(720, 550, 160, 42), Text = "Cancel / Close" };
            foreach (var b in new[] { checkBtn, nextBtn, closeBtn })
                b.Font = new Font(FontFamily.GenericSansSerif, 11f, FontStyle.Bold);
            checkBtn.Click += (s, e) => CheckText();
            nextBtn.Click += (s, e) => StartTrial();
            closeBtn.Click += (s, e) => Close();

            Controls.AddRange(new Control[]
            {
                _trialLabel, _nonceLabel, instr, _statusLabel, _shapes, sample, _textBox,
                _resultLabel, checkBtn, nextBtn, closeBtn
            });

            if (_logger != null)
                _logger.Log("session", "seed", _seedUsed.ToString(), "platform", "windows");
        }

        // MARK: Trial logic

        private string MakeNonce()
        {
            var sb = new StringBuilder(6);
            for (int i = 0; i < 6; i++) sb.Append(NonceChars[_rng.Below(NonceChars.Length)]);
            return sb.ToString();
        }

        public void StartTrial()
        {
            _trialIndex++;
            _nonce = MakeNonce();

            var items = new Item[4];
            items[0] = new Item { Kind = ShapeKind.Circle, Color = Palette.Green, ColorName = "green", IsTarget = true };
            var shapes = new[] { ShapeKind.Square, ShapeKind.Triangle, ShapeKind.Diamond };
            _rng.Shuffle(shapes);
            var colors = new[]
            {
                Tuple.Create(Palette.Red, "red"),
                Tuple.Create(Palette.Blue, "blue"),
                Tuple.Create(Palette.Yellow, "yellow")
            };
            _rng.Shuffle(colors);
            for (int i = 0; i < 3; i++)
                items[i + 1] = new Item { Kind = shapes[i], Color = colors[i].Item1, ColorName = colors[i].Item2, IsTarget = false };
            _rng.Shuffle(items);
            _items = items;
            _targetSlot = Array.FindIndex(items, it => it.IsTarget);

            _trialLabel.Text = "Trial " + _trialIndex;
            _nonceLabel.Text = "NONCE: " + _nonce;
            _statusLabel.Text = "";
            _statusLabel.BackColor = Color.White;
            _resultLabel.Text = "";
            _textBox.Text = "";
            _shapes.Items = _items;
            _shapes.Invalidate();

            string layout = string.Join(",", _items.Select((it, i) =>
                i + ":" + it.ColorName + "-" + it.Kind.ToString().ToLowerInvariant()));
            if (_logger != null)
                _logger.Log("trial", "trial", _trialIndex, "nonce", _nonce,
                            "target", "green circle", "target_slot", _targetSlot, "layout", layout);
        }

        private void ShapeClicked(int slot)
        {
            bool hit = slot == _targetSlot;
            _statusLabel.Text = hit ? "TARGET HIT" : "WRONG TARGET";
            _statusLabel.ForeColor = hit ? Palette.Green : Palette.Red;
            _statusLabel.BackColor = hit ? Color.FromArgb(209, 245, 209) : Color.FromArgb(252, 214, 214);
            if (_logger != null)
                _logger.Log(hit ? "hit" : "wrong", "trial", _trialIndex, "nonce", _nonce,
                            "slot", slot, "target_slot", _targetSlot);
        }

        private static int GraphemeCount(string s) { return new StringInfo(s).LengthInTextElements; }

        private void CheckText()
        {
            string got = _textBox.Text;
            bool matched = got == SampleText;
            int expected = GraphemeCount(SampleText);
            int gotLen = GraphemeCount(got);
            if (matched)
            {
                _resultLabel.Text = "MATCHED — " + gotLen + " characters";
                _resultLabel.ForeColor = Palette.Green;
            }
            else
            {
                _resultLabel.Text = "MISMATCH — expected " + expected + ", got " + gotLen;
                _resultLabel.ForeColor = Palette.Red;
            }
            if (_logger != null)
                _logger.Log("text_check", "trial", _trialIndex, "nonce", _nonce,
                            "matched", matched, "expected_len", expected, "got_len", gotLen);
        }
    }

    // MARK: - Entry point

    internal static class Program
    {
        [STAThread]
        static void Main(string[] args)
        {
            ulong? seed = null;
            string evidencePath = null;
            for (int i = 0; i < args.Length; i++)
            {
                string a = args[i];
                if (a == "--seed" && i + 1 < args.Length) { seed = ulong.Parse(args[i + 1]); i++; }
                else if (a == "--evidence-file" && i + 1 < args.Length) { evidencePath = args[i + 1]; i++; }
                else if (a.StartsWith("--seed=")) seed = ulong.Parse(a.Substring(7));
                else if (a.StartsWith("--evidence-file=")) evidencePath = a.Substring(16);
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            var form = new MainForm(evidencePath, seed);
            // first trial starts when the form is shown
            form.Shown += (s, e) => ((MainForm)s).StartTrial();
            Application.Run(form);
        }
    }
}
