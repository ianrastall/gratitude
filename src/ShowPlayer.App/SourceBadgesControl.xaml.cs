using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI.ViewManagement;

namespace Jupiter.ShowPlayer.App;

/// <summary>Text-labelled recording badges; colors identify type, not rank.</summary>
public sealed partial class SourceBadgesControl : UserControl
{
    public static readonly DependencyProperty LabelsProperty = DependencyProperty.Register(
        nameof(Labels), typeof(string), typeof(SourceBadgesControl),
        new PropertyMetadata("", (d, _) => ((SourceBadgesControl)d).Render()));
    public string Labels { get => (string)GetValue(LabelsProperty); set => SetValue(LabelsProperty, value); }
    private readonly AccessibilitySettings _accessibility = new();
    private readonly UISettings _uiSettings = new();

    public SourceBadgesControl()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += (_, _) => _uiSettings.ColorValuesChanged -= OnColorsChanged;
        ActualThemeChanged += (_, _) => Render();
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _uiSettings.ColorValuesChanged += OnColorsChanged;
        Render();
    }
    private void OnColorsChanged(UISettings sender, object args) => DispatcherQueue.TryEnqueue(Render);

    private void Render()
    {
        if (Rows is null) return;
        Rows.Children.Clear();
        string labels = Labels ?? "";
        string[] tokens = labels.Split(" · ")[0].Split('|', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        StackPanel? row = null;
        for (int i = 0; i < tokens.Length; i++)
        {
            if (i % 3 == 0) { row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 }; Rows.Children.Add(row); }
            string token = tokens[i];
            string key = token switch { "SBD" => "SourceSbdBrush", "MTX" => "SourceMtxBrush", "AUD" => "SourceAudBrush", _ => "SourceModifierBrush" };
            bool neutral = token == "UNKNOWN" || _accessibility.HighContrast;
            var text = new TextBlock { Text = token, Style = (Style)Application.Current.Resources["JupiterCaptionTextStyle"] };
            text.Foreground = (Brush)(neutral ? Application.Current.Resources["JupiterPrimaryTextBrush"] : Resources["SourceBadgeTextBrush"]);
            var badge = new Border
            {
                Padding = new Thickness(8, 4, 8, 4),
                CornerRadius = (CornerRadius)Application.Current.Resources["ControlCornerRadius"],
                BorderThickness = new Thickness(1),
                BorderBrush = (Brush)(neutral ? Application.Current.Resources["JupiterControlBoundaryBrush"] : Resources[key]),
                Background = (Brush)(neutral ? Application.Current.Resources["JupiterRaisedBrush"] : Resources[key]),
                Child = text
            };
            AutomationProperties.SetName(badge, token);
            ToolTipService.SetToolTip(badge, labels);
            row!.Children.Add(badge);
        }
        if (labels.Contains("inferred", StringComparison.OrdinalIgnoreCase))
            Rows.Children.Add(new TextBlock { Text = "Type inferred from identifier", Style = (Style)Application.Current.Resources["JupiterCaptionTextStyle"] });
    }
}
