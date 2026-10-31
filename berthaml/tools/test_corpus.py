"""Regression tests for conversions that can change historical meaning."""
import copy
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from lxml import etree, html

import build_corpus as corpus
from collect_jerrybase_facts import program
from validate import validate_memory


def gd(key, venue='Fillmore East', titles=()):
    return {'key': key, ':venue': venue, ':city': 'New York', ':sets': [
        {':songs': [{':name': name} for name in titles]}]}


def jb(day, number, titles=(), venue='Fillmore East, New York, NY'):
    return dict(date_label=day + ' [Fri]', url=f'https://jerrybase.com/events/{day.replace("-", "")}-{number:02}',
                venue_label=venue, sections=[dict(items=[dict(title=t) for t in titles])])


class CorpusTests(unittest.TestCase):
    def test_prospective_validation_checks_cross_file_references(self):
        files, _ = corpus.build()
        path = next(p for p in files if p.startswith('data/shows/'))
        show = etree.fromstring(files[path])
        show.find('b:venue', corpus.N).set('ref', 'v-missing-test-venue')
        files[path] = etree.tostring(show)
        with redirect_stdout(io.StringIO()):
            report = validate_memory(files, corpus.ROOT / 'schemas/berthaml.xsd', corpus.ROOT / 'data')
        self.assertEqual(report.errors, 1)

    def test_fuzzy_date_uses_label_not_url_day(self):
        event = jb('1970-08-00', 3)
        event['url'] = 'https://jerrybase.com/events/19700801-03'
        self.assertEqual(corpus.show_date(event['date_label']), '1970-08')
        self.assertEqual(corpus.show_id(event), 'gd1970-08-xx-jb03')

    def test_unknown_locality_is_not_assumed_american(self):
        self.assertEqual(corpus.locality('Unknown'), ('Unknown', {}))
        self.assertEqual(corpus.locality('The Strand Lyceum, London, England')[1],
                         dict(city='London', country='GB'))

    def test_no_early_late_guess_from_source_order(self):
        left = [gd('1970/01/02/0'), gd('1970/01/02/1')]
        right = [jb('1970-01-02', 1), jb('1970-01-02', 2)]
        matches, unresolved = corpus.reconcile(left, right)
        self.assertEqual(matches, [])
        self.assertEqual(len(unresolved[0]['gdshowsdb']), 2)

    def test_song_sequence_resolves_venue_ambiguity(self):
        matches, _ = corpus.reconcile([gd('1970/01/02/0', titles=['Dark Star']),
                                       gd('1970/01/02/1', titles=['Easy Wind'])],
                                      [jb('1970-01-02', 1, ['Easy Wind']),
                                       jb('1970-01-02', 2, ['Dark Star'])])
        self.assertEqual({(a['key'], b['url'].rsplit('-', 1)[1]) for a, b, _ in matches},
                         {('1970/01/02/0', '02'), ('1970/01/02/1', '01')})

    def test_comma_titles_modes_and_unmodeled_context(self):
        cell = html.fromstring('<td><b>Show: </b><span class="song-context">Acoustic:</span>'
                               "Oh Babe, It Ain't No Lie &gt; Ripple, "
                               '<span class="song-context">Electric:</span>Jack Straw'
                               '<span class="song-context">guest information</span>'
                               '<br><b>Encore: </b>Box Of Rain</td>')
        sections, contexts = program(cell, {"Oh Babe, It Ain't No Lie"})
        self.assertEqual([x['title'] for x in sections[0]['items']],
                         ["Oh Babe, It Ain't No Lie", 'Ripple', 'Jack Straw'])
        self.assertEqual(sections[0]['items'][0]['transition'], 'segue')
        self.assertEqual(sections[0]['items'][1]['transition'], 'unknown')
        self.assertEqual(sections[0]['items'][2]['mode'], 'electric')
        self.assertEqual(contexts, 1)
        self.assertEqual(sections[1]['items'][0]['title'], 'Box Of Rain')

    def test_inferred_recording_type_and_combined_credit_not_asserted(self):
        item = etree.fromstring(b'<recording provider="etree" key="etree-1" inferredType="true" '
                                b'type="SBD" title="test" sourceRefs="seed">'
                                b'<creditText role="transfer-mastering">Person</creditText>'
                                b'<creditText role="recording">Taper</creditText></recording>')
        result = corpus.recording(item)
        self.assertEqual(result.get('kind'), 'unknown')
        self.assertEqual(len(result.findall('b:credit', corpus.N)), 1)
        self.assertIn('transfer-mastering', ''.join(result.itertext()))

    def test_publish_refuses_local_changes_before_any_mutation(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(corpus, 'ROOT', Path(directory)):
            root = Path(directory)
            (root / 'data').mkdir()
            path = root / 'data/test.xml'
            original = b'original'
            path.write_bytes(b'local edit')
            (root / 'corpus-manifest.json').write_text(json.dumps(dict(files={'data/test.xml': corpus.digest(original)})))
            with self.assertRaisesRegex(ValueError, 'Preserve the local edit'):
                corpus.publish({'data/test.xml': b'replacement', 'reports/new.json': b'new'}, destination=root)
            self.assertEqual(path.read_bytes(), b'local edit')
            self.assertFalse((root / 'reports/new.json').exists())

    def test_current_source_accounting_and_determinism(self):
        first, stats = corpus.build()
        second, again = corpus.build()
        self.assertEqual(first, second)
        self.assertEqual(stats, again)
        self.assertEqual(stats['shows'], 2234)
        self.assertEqual(stats['supplied_recordings'],
                         stats['assigned_recording_references'] + stats['unassigned_recording_references'])
        self.assertEqual(stats['show_status'].get('reviewed', 0), 0)
        self.assertEqual(stats['show_status'].get('verified', 0), 0)


if __name__ == '__main__':
    unittest.main()
