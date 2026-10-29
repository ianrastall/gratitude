using Jupiter.ShowPlayer.Interop;
using Jupiter.ShowPlayer.Interop.Messages;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
namespace Jupiter.ShowPlayer.App;
public sealed partial class RecordingPrioritiesControl : UserControl
{
 private AudioCore? _core;
 public RecordingPrioritiesControl(){InitializeComponent();Unloaded+=(_,_)=>App.Current.RecordingPrioritiesChanged-=Refresh;}
 public void Configure(AudioCore core){_core=core;App.Current.RecordingPrioritiesChanged+=Refresh;Refresh();}
 private void Refresh(){if(_core is null)return;try{var p=_core.GetPriorities();string people=p.People.Count==0?"No people preference":string.Join(" > ",p.People);string types=string.Join(" > ",p.Sources.Where(s=>s!="UNKNOWN"));Summary.Text="Priorities: "+(p.PeopleFirst?people+" | "+types:types+" | "+people)+(p.Modifiers.Count==0?"":" | "+string.Join(" > ",p.Modifiers));ToolTipService.SetToolTip(OpenButton,Summary.Text);}catch(AudioCoreException e){Problem.Text=e.Message;}}
 private void OnOpen(object sender,RoutedEventArgs e){if(_core is null)return;var p=_core.GetPriorities();SourcesBox.Text=string.Join(", ",p.Sources);PeopleBox.Text=string.Join(", ",p.People);ModifiersBox.Text=string.Join(", ",p.Modifiers);FirstBox.SelectedIndex=p.PeopleFirst?1:0;Problem.Text="";}
 public void OpenEditor(){OnOpen(this,new RoutedEventArgs());Editor.ShowAt(OpenButton);}
 private void OnApply(object sender,RoutedEventArgs e){if(_core is null)return;var p=new RecordingPriorities{PeopleFirst=FirstBox.SelectedIndex==1};p.Sources.Add(SourcesBox.Text.Split(',',StringSplitOptions.TrimEntries|StringSplitOptions.RemoveEmptyEntries));p.People.Add(PeopleBox.Text.Split(',',StringSplitOptions.TrimEntries|StringSplitOptions.RemoveEmptyEntries));p.Modifiers.Add(ModifiersBox.Text.Split(',',StringSplitOptions.TrimEntries|StringSplitOptions.RemoveEmptyEntries));try{_core.SetPriorities(p);App.Current.NotifyRecordingPrioritiesChanged();Editor.Hide();}catch(AudioCoreException ex){Problem.Text=ex.Message;}}
 private void OnReset(object sender,RoutedEventArgs e){if(_core is null)return;var p=new RecordingPriorities();p.People.Add(new[]{"Betty","Miller"});try{_core.SetPriorities(p);App.Current.NotifyRecordingPrioritiesChanged();OnOpen(this,new RoutedEventArgs());}catch(AudioCoreException ex){Problem.Text=ex.Message;}}
}
