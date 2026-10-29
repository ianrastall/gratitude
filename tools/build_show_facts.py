"""Fetch attributed show/recording facts for the Rust XML catalogue compiler.

Python is acquisition glue. Source classification, ranking and XML generation
belong to the Rust core. Editorial reviews and book text are not distributed.
"""
import datetime as dt
import hashlib
import html
from html.parser import HTMLParser
import io
import json
from pathlib import Path
import re
import time
import urllib.parse
import urllib.request
import zipfile
import yaml

ROOT=Path(__file__).resolve().parents[1]
CACHE=ROOT/'work/catalogue-sources'
CACHE.mkdir(parents=True,exist_ok=True)

def fetch(url,name):
    path=CACHE/name
    if path.exists():return path.read_bytes()
    for attempt in range(3):
        try:
            data=urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'JupiterShowPlayer/0.2 catalogue-builder'}),timeout=60).read()
            path.write_bytes(data)
            return data
        except Exception:
            if attempt==2:raise
            time.sleep(2+attempt)

def clean(text):
    return ' '.join(html.unescape(re.sub('<[^>]+>',' ',str(text or ''))).split())

class Blocks(HTMLParser):
    def __init__(self):super().__init__();self.blocks=[];self.text=[];self.links=[]
    def handle_starttag(self,tag,attrs):
        if tag=='p':self.text=[];self.links=[]
        if tag=='a':self.links += [v for k,v in attrs if k=='href']
    def handle_data(self,text):self.text.append(text)
    def handle_endtag(self,tag):
        if tag=='p':self.blocks.append((clean(''.join(self.text)),self.links.copy()));self.text=[];self.links=[]

def date_of(value):
    value=str(value)
    m=re.search(r'((?:19|20)\d{2})[-/](\d{1,2})[-/](\d{1,2})',value,re.I) or re.search(r'gd(\d{2})[-/](\d{1,2})[-/](\d{1,2})',value,re.I)
    if m:y,month,day=map(int,m.groups())
    else:
        m=re.search(r'(?<!\d)(\d{1,2})[-/](\d{1,2})[-/]((?:19|20)?\d{2})(?!\d)',value)
        if not m:return None
        month,day,y=map(int,m.groups())
    y=y+1900 if y<100 else y
    try:d=dt.date(y,month,day)
    except ValueError:return None
    return d.isoformat() if 1967<=d.year<=1995 else None

def source(id,url,scope,data):
    return dict(id=id,url=url,scope=scope,sha256=hashlib.sha256(data).hexdigest())

def main():
    sources=[];shows={};recordings={};unplaced=[]
    def show(date):
        return shows.setdefault(date,dict(date=date,venue='',city='',state='',country='',performances=[],references=[],recordings=[]))
    revision=json.loads(fetch('https://api.github.com/repos/jefmsmit/gdshowsdb/commits/main','gdshowsdb-commit.json'))['sha']
    raw=fetch('https://codeload.github.com/jefmsmit/gdshowsdb/zip/'+revision,'gdshowsdb-'+revision+'.zip')
    sources.append(source('gdshowsdb','https://github.com/jefmsmit/gdshowsdb',f'MIT-licensed show/setlist snapshot; revision {revision}',raw))
    archive=zipfile.ZipFile(io.BytesIO(raw))
    prefix=archive.namelist()[0]
    license_text=archive.read(prefix+'LICENSE.txt')
    (ROOT/'data/gdshowsdb-LICENSE.txt').write_bytes(license_text)
    for year in range(1967,1996):
        records=yaml.safe_load(archive.read(prefix+f'data/gdshowsdb/{year}.yaml'))
        for key,entry in records.items():
            date=date_of(key)
            if not date:
                unplaced.append(dict(source='gdshowsdb',date_text=str(key),reason='Unknown or invalid calendar date'));continue
            s=show(date)
            p=dict(id=entry.get(':uuid',str(key)),venue=str(entry.get(':venue') or ''),city=str(entry.get(':city') or ''),state=str(entry.get(':state') or ''),country=str(entry.get(':country') or ''),sets=[])
            for i,st in enumerate(entry.get(':sets') or []):
                p['sets'].append(dict(number=i+1,songs=[dict(name=clean(song.get(':name')),segue=bool(song.get(':segued'))) for song in (st.get(':songs') or [])]))
            s['performances'].append(p)
            for field in ['venue','city','state','country']:
                if not s[field]:s[field]=p[field]
            s['references'].append('https://github.com/jefmsmit/gdshowsdb/blob/'+revision+f'/data/gdshowsdb/{year}.yaml')
    print('Show dates:',len(shows),flush=True)
    archive_pages=[]
    for first,last in [(1967,1976),(1977,1986),(1987,1995)]:
      query=f'collection:GratefulDead AND date:[{first}-01-01T00:00:00Z TO {last}-12-31T23:59:59Z]'
      page=1;found=1;count=0
      while count<found:
        params=[('q',query),('rows','1000'),('page',str(page)),('output','json'),('sort[]','identifier asc')]+[('fl[]',f) for f in ['identifier','title','date','venue','coverage','source','taper','transferer','lineage','collection']]
        filename=f'archive-{first}-{last}-{page}.json';archive_pages.append(filename)
        data=fetch('https://archive.org/advancedsearch.php?'+urllib.parse.urlencode(params),filename)
        response=json.loads(data)['response'];found=response['numFound'];docs=response['docs']
        if found>10000:raise RuntimeError('Split Archive date bucket below its 10,000 result limit')
        if not docs:raise RuntimeError('Archive index ended before advertised total')
        for item in docs:
            key=item['identifier'];date=date_of(item.get('date','')) or date_of(key)
            if not date:
                unplaced.append(dict(source='archive-index',date_text=key,reason='No unambiguous 1967–1995 date'));continue
            def text(field):
                value=item.get(field,'');return clean(' | '.join(map(str,value)) if isinstance(value,list) else value)
            r=dict(key=key,source='Archive.org',title=text('title') or key,date=date,source_text=text('source'),taper=text('taper'),transferer=text('transferer'),lineage=text('lineage'),hints='',detail='Archive metadata snapshot; availability checked before download',provenance=['archive-index'],references=[])
            recordings[('Archive.org',key)]=r
            s=show(date)
            if not s['venue']:s['venue']=text('venue')
            if not s['city']:s['city']=text('coverage')
        count+=len(docs);page+=1
        print(f'Archive {first}–{last}: {count}/{found}',flush=True)
    sources.append(dict(id='archive-index',url='https://archive.org/details/GratefulDead',scope='Public metadata snapshot, not a download permission assertion',sha256=hashlib.sha256(''.join((CACHE/name).read_text() for name in archive_pages).encode()).hexdigest()))
    for file,provider in [('gd_torrents.json','etree-snapshot'),('grateful_dead_torrent_urls.txt','archive-url-snapshot')]:
        data=(ROOT/'data'/file).read_bytes();sources.append(source(provider,'user-supplied:'+file,'User-supplied historical listing',data))
        if file.endswith('.json'):
            for item in json.loads(data):
                date=date_of(item['title'])
                if not date:
                    unplaced.append(dict(source=provider,date_text=item['title'],reason='No unambiguous 1967–1995 date'));continue
                key='etree-'+str(item['id'])
                recordings[('etree',key)]=dict(key=key,source='etree',title=item['title'],date=date,source_text='',taper='',transferer='',lineage='',hints='',detail=f"Saved listing; {item.get('seeds',0)} seeds at capture (not live)",provenance=[provider],references=[])
        else:
            for line in data.decode().splitlines():
                m=re.search('/items/([^/]+)/',line)
                if not m:continue
                key=m[1];date=date_of(key)
                if not date:
                    unplaced.append(dict(source=provider,date_text=key,reason='No unambiguous 1967–1995 date'));continue
                r=recordings.setdefault(('Archive.org',key),dict(key=key,source='Archive.org',title=key,date=date,source_text='',taper='',transferer='',lineage='',hints='',detail='Historical listing; availability checked before download',provenance=[],references=[]))
                if provider not in r['provenance']:r['provenance'].append(provider)
    posts=[]
    for page in [1,2]:
        data=fetch(f'https://public-api.wordpress.com/rest/v1.1/sites/infinitedead.net/posts/?number=100&page={page}&fields=ID,title,URL,content',f'infinite-dead-{page}.json')
        response=json.loads(data);posts+=response['posts']
    page=5
    while len(posts)<response['found']:
        data=fetch(f'https://public-api.wordpress.com/rest/v1.1/sites/infinitedead.net/posts/?number=50&page={page}&fields=ID,title,URL,content',f'infinite-smaller{page}.json')
        response=json.loads(data)
        if not response['posts']:raise RuntimeError('Infinite Dead pagination ended early')
        posts+=response['posts']
        print(f'Infinite Dead posts: {len(posts)}/{response["found"]}',flush=True)
        page+=1
    if len({p['ID'] for p in posts})!=len(posts):raise RuntimeError('Duplicate Infinite Dead posts across pages')
    for post in posts:
        date=date_of(clean(post['title']))
        if not date:continue
        s=show(date)
        if post['URL'] not in s['references']:s['references'].append(post['URL'])
        parser=Blocks();parser.feed(post['content']);previous=''
        for text,links in parser.blocks:
            for link in links:
                m=re.search(r'archive\.org/(?:details|download)/([^/?#]+)',link)
                if not m:continue
                key=urllib.parse.unquote(m[1])
                r=recordings.setdefault(('Archive.org',key),dict(key=key,source='Archive.org',title=key,date=date,source_text='',taper='',transferer='',lineage='',hints='',detail='Linked by Infinite Dead; availability checked before download',provenance=[],references=[]))
                # This text is only input evidence for Rust fact extraction. The
                # compiler emits classifications/credits, never review prose.
                context=previous if text.strip().startswith(('http://','https://')) else text
                r['hints']+=' '+context
                if 'infinite-dead' not in r['provenance']:r['provenance'].append('infinite-dead')
                if post['URL'] not in r['references']:r['references'].append(post['URL'])
            if text:previous=text
    sources.append(dict(id='infinite-dead',url='https://infinitedead.net/',scope=f'{len(posts)} public posts; July and October source links/classifications only; editorial prose not included',sha256=hashlib.sha256(json.dumps(posts,sort_keys=True).encode()).hexdigest()))
    sources.append(dict(id='live-dead-reference',url='urn:isbn:9781478027614',scope='John Brackett, Live Dead (Duke University Press): contextual reference supplied as PDF; no catalogue facts or book text extracted',sha256=''))
    for r in recordings.values():show(r['date'])['recordings'].append(r)
    for s in shows.values():s['references']=list(dict.fromkeys(s['references']))
    facts=dict(schema_version=1,start_year=1967,end_year=1995,captured_date='2026-10-29',sources=sources,shows=[shows[k] for k in sorted(shows)],unplaced=unplaced)
    output=ROOT/'work/show-facts.json';output.write_text(json.dumps(facts,ensure_ascii=False),encoding='utf-8')
    print(json.dumps(dict(show_dates=len(shows),performances=sum(len(s['performances']) for s in shows.values()),recordings=len(recordings),unplaced=len(unplaced),facts=str(output))),flush=True)

if __name__=='__main__':main()
