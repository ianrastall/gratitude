using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Jupiter.ShowPlayer.App;

public sealed partial class RecordingPrioritiesControl : UserControl
{
    private AudioCore? _core;
    private bool _refreshing;

    public RecordingPrioritiesControl()
    {
        InitializeComponent();
        Unloaded += (_, _) => App.Current.RecordingPrioritiesChanged -= Refresh;
    }

    public void Configure(AudioCore core)
    {
        _core = core;
        App.Current.RecordingPrioritiesChanged += Refresh;
        Refresh();
    }

    private void Refresh()
    {
        if (_core is null) return;
        _refreshing = true;
        try { PreferenceBox.SelectedIndex = _core.GetPriorities().Sources[0] == "MTX" ? 1 : 0; }
        finally { _refreshing = false; }
    }

    public void OpenEditor() => PreferenceBox.IsDropDownOpen = true;

    private void OnPreferenceChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_refreshing || _core is null || PreferenceBox.SelectedIndex < 0) return;
        var preference = new RecordingPriorities();
        preference.Sources.Add(PreferenceBox.SelectedIndex == 1
            ? new[] { "MTX", "SBD" } : new[] { "SBD", "MTX" });
        try
        {
            _core.SetPriorities(preference);
            App.Current.NotifyRecordingPrioritiesChanged();
        }
        catch (AudioCoreException ex)
        {
            AppLog.Write($"Source preference could not be saved: {ex.Message}");
            Refresh();
            ToolTipService.SetToolTip(PreferenceBox, ex.Message);
        }
    }
}
