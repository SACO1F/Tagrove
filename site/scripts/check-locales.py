"""Check both static language pages and their matching media references."""
from pathlib import Path
from html.parser import HTMLParser
import re, tempfile, subprocess
site=Path(__file__).resolve().parents[1]
class Page(HTMLParser):
 def __init__(self):super().__init__();self.ids=[];self.refs=[];self.lang=None;self.switch=None;self.videos=[];self.anchors=[];self.stack=[];self.errors=[]
 def handle_starttag(self,t,attrs):
  a=dict(attrs)
  if t=='html':self.lang=a['lang']
  if a.get('class')=='language-switch':self.switch=a
  if 'id'in a:self.ids.append(a['id'])
  if t=='video':self.videos.append(a['data-src']);assert 'muted'in a and 'playsinline'in a
  if a.get('href','').startswith('#'):self.anchors.append(a['href'][1:])
  for k in ['href','src','data-src']:
   if a.get(k,'').startswith('assets/'):self.refs.append(a[k])
  if t not in ['meta','link','img','br','hr','input','path','circle','rect','source']:self.stack.append(t)
 def handle_endtag(self,t):
  if t in ['meta','link','img','br','hr','input','path','circle','rect','source']:return
  if not self.stack or self.stack.pop()!=t:self.errors.append(t)
pages=[]
for name,lang,suffix,target in [('index.html','en','en','zh.html'),('zh.html','zh-CN','zh','index.html')]:
 source=(site/name).read_text(encoding='utf-8');p=Page();p.feed(source)
 assert p.lang==lang
 assert p.switch['href']==target
 assert (site/target).is_file()
 assert not p.errors and not p.stack,(name,p.errors,p.stack)
 assert len(p.ids)==len(set(p.ids))
 assert set(p.anchors)<=set(p.ids)
 assert all((site/ref).is_file() for ref in p.refs)
 assert len(p.videos)==4 and all(x.endswith('-'+suffix+'.mp4')for x in p.videos)
 assert all('-'+suffix+'.' in x for x in p.refs if x.startswith('assets/ui/'))
 for i,script in enumerate(re.findall(r'<script>(.*?)</script>',source,re.S)):
  f=Path(tempfile.gettempdir())/(name+f'-{i}.cjs');f.write_text(script,encoding='utf-8')
  subprocess.run(['node','--check',str(f)],check=True,capture_output=True)
 pages.append(p)
 print('PASS',name,': correct language switch, localized media, anchors, HTML structure and script syntax.')
assert set(pages[0].ids)==set(pages[1].ids),'Section/demo IDs must stay aligned across languages'
print('PASS both pages have identical anchor IDs for language switching at the same section.')
