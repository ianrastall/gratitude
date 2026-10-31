"""Checks for source boundaries, edition identity, unknowns and playable exports."""
import gzip
import json
from pathlib import Path
import unittest

from lxml import etree

from collect_playlists import extract
from playlist_data import (N, NS, cache_name, candidate_covers, duration, export_playlists,
                           recording_date, safe_file, plain, title_key, track_title)
from validate import check_track_list

ROOT = Path(__file__).resolve().parents[1]


class PlaylistTests(unittest.TestCase):
    def test_companion_notes_are_plain_text_and_preserve_signal_paths(self):
        self.assertEqual(plain('SBD &gt; DAT<br /><b>Transfer</b> &lt; 10 V'),
                         'SBD > DAT\nTransfer < 10 V')
        self.assertEqual(plain(['One', 'Two']), 'One; Two')

    def test_original_audio_only_and_source_order(self):
        source = dict(metadata=dict(identifier='gd1977-05-08.test'), files=[
            dict(name='gd77d2t02.flac', source='original', title='Second', length='02:03.125'),
            dict(name='gd77d2t01.flac', source='original', title='First', length='4.5'),
            dict(name='gd77d2t01_vbr.mp3', source='derivative', title='First'),
            dict(name='show-notes.txt', source='original')])
        result = extract('gd1977-05-08.test', json.dumps(source).encode(), '2026-10-31T02:00:00Z')
        self.assertEqual(len(result['variants']), 1)
        variant = result['variants'][0]
        self.assertEqual([t['title'] for t in variant['tracks']], ['First', 'Second'])
        self.assertEqual(variant['tracks'][1]['duration'], 'PT123.125S')
        self.assertEqual(variant['tracks'][0]['disc'], 2)
        self.assertFalse(variant['order_uncertain'])

    def test_unmeasured_timing_remains_absent(self):
        for value in [None, '', 'unknown', '-2', 'NaN', 'Infinity', '1:-30', '1:60']:
            self.assertIsNone(duration(value))
        self.assertEqual(duration('1:02:03.50'), 'PT3723.5S')
        self.assertEqual(duration('0'), 'PT0S')

    def test_no_ordinal_means_order_is_a_review_question(self):
        source = dict(metadata=dict(identifier='test'), files=[
            dict(name='second.flac', source='original'), dict(name='first.flac', source='original')])
        variant = extract('test', json.dumps(source).encode(), 'now')['variants'][0]
        self.assertTrue(variant['order_uncertain'])
        self.assertEqual(variant['order_basis'], 'natural filename order')

    def test_source_date_disagreement_not_hidden_by_catalogue_group(self):
        self.assertEqual(recording_date('Archive.org', 'gd1977-05-08.foo', ''), '1977-05-08')
        self.assertIsNone(recording_date('Archive.org', 'gd1967-00-00.foo', ''))
        self.assertIsNone(recording_date('etree', 'etree-123', 'gd1977-05-08 plus gd1977-05-09'))

    def test_m3u_paths_cannot_escape_or_inject_directives(self):
        for value in ['../audio.flac', '/audio.flac', 'D:/audio.flac', 'disc\\file.flac',
                      '#EXTINF:10,test', 'file.flac\n#bad']:
            self.assertFalse(safe_file(value))
        self.assertTrue(safe_file('disc1/01 Bertha.flac'))

    def test_repeated_song_is_ambiguous_but_unique_medley_can_link(self):
        show = etree.fromstring(f'<show xmlns="{NS}"><program><set>'
                                '<performance id="p1" song="s-bertha"/>'
                                '<performance id="p2" song="s-drums"/>'
                                '<performance id="p3" song="s-bertha"/>'
                                '</set></program></show>')
        songs = {sid: etree.fromstring(f'<song xmlns="{NS}"><title>{name}</title></song>')
                 for sid, name in [('s-bertha', 'Bertha'), ('s-drums', 'Drums')]}
        self.assertEqual(candidate_covers('Bertha', show, songs), [])
        self.assertEqual(candidate_covers('Bertha > Drums > Bertha', show, songs), ['p1', 'p2', 'p3'])

    def test_titles_compare_past_numbering_and_spelling(self):
        self.assertEqual(track_title('12 Cold Jordon ->'), 'Cold Jordon')
        self.assertEqual(track_title('gd81-02-26 03 Little Red Rooster'), 'Little Red Rooster')
        self.assertEqual(track_title("E: Uncle John's Band~~"), "Uncle John's Band")
        self.assertEqual(track_title('Scarlet Begonias > Fire'), 'Scarlet Begonias > Fire')
        self.assertEqual(title_key('The Promised Land'), title_key('Promised Land'))
        self.assertEqual(title_key('Around & Around'), title_key('Around And Around'))
        self.assertEqual(title_key("Goin' Down The Road Feelin' Bad"), title_key('Going Down The Road Feeling Bad'))
        self.assertEqual(title_key("Franklin's Tower"), title_key('Franklins Tower'))
        self.assertNotEqual(title_key('Other One'), title_key('Other Ones'))

    def test_alternate_titles_link_tracks(self):
        show = etree.fromstring(f'<show xmlns="{NS}"><program><set>'
                                '<performance id="p1" song="s-minglewood-blues"/></set></program></show>')
        songs = {'s-minglewood-blues': etree.fromstring(
            f'<song xmlns="{NS}"><title>Minglewood Blues</title><altTitle>New Minglewood Blues</altTitle></song>')}
        self.assertEqual(candidate_covers('03 New Minglewood Blues ->', show, songs), ['p1'])

    def test_candidate_mapping_requires_real_performance_refs(self):
        class Report:
            def __init__(self):
                self.errors = []
            def error(self, path, line, message):
                self.errors.append(message)
        report = Report()
        track = etree.fromstring(f'<track xmlns="{NS}" n="1" title="Unknown" mappingStatus="candidate"/>')
        check_track_list(Path('test.xml'), [track], report, 'test')
        self.assertEqual(len(report.errors), 1)

    def test_cd_and_vinyl_tracklists_stay_separate(self):
        regs = {name: etree.fromstring(f'<{name} xmlns="{NS}"/>') for name in ['songs', 'venues']}
        regs['releases'] = etree.fromstring(f'<releases xmlns="{NS}"><record status="draft"/>'
            '<release id="rel-test"><title>Test</title>'
            '<edition id="ed-test-cd" date="2020" format="cd">'
            '<track disc="1" n="1" title="One > Two" duration="PT300S"/></edition>'
            '<edition id="ed-test-lp" date="2021" format="lp">'
            '<track disc="1" n="1" title="One" duration="PT120S"/>'
            '<track disc="2" n="1" title="Two" duration="PT180S"/></edition>'
            '</release></releases>')
        outputs, stats = export_playlists([], {}, regs)
        cd = json.loads(outputs['playlists/official/ed-test-cd.json'])
        lp = json.loads(outputs['playlists/official/ed-test-lp.json'])
        self.assertEqual(len(cd['tracks']), 1)
        self.assertEqual(len(lp['tracks']), 2)
        self.assertFalse(any(p.endswith('.m3u8') for p in outputs))
        self.assertEqual(stats['known_official_editions_missing'], 0)

    def test_actual_cornell_sources_have_different_boundaries(self):
        counts = []
        for identifier in ['gd1977-05-08.137569.mtx.dusborne.flac24',
                           'gd1977-05-08.111493.mtx.seamons.sbeok.flac16']:
            source = json.loads(gzip.decompress((ROOT / 'sources/recording-playlists' / cache_name(identifier)).read_bytes()))
            counts.append(len(source['variants'][0]['tracks']))
        self.assertEqual(counts, [22, 21])


if __name__ == '__main__':
    unittest.main()
