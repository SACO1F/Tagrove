"""Build zh.html from index.html and reviewed Chinese copy.
Run from site/: python scripts/build-chinese.py
Fail on new untranslated visible copy so the two pages cannot silently drift.
"""
from pathlib import Path
from html.parser import HTMLParser
import html,json,re
site=Path(__file__).resolve().parents[1]
copy=json.loads((site/'scripts/copy-zh.json').read_text(encoding='utf-8'))
attributes={
'A local workspace for files, images and web references. Keep originals in place, connect them with tags, and find what you need.':'本地文件、图片与网页参考工作台。原件留在原处，用标签联系资料，需要时随手找回。',
'Page navigation':'页面导航','Demo tags':'演示标签','Switch to Chinese':'切换为英文',
'Sample reference: glass tower at dusk':'示意资料：黄昏中的玻璃塔楼',
'Sample reference: concrete facade and window detail':'示意资料：混凝土立面与窗套细部',
'Sample reference: concrete, oak, terrazzo and metal samples':'示意资料：混凝土、橡木、水磨石与金属样板',
'Tagrove screenshot showing multiple tags on a Markdown file and its editable content in the inspector':'拾签真实界面：带有多个标签的 Markdown 文档，右侧详情展示可编辑的中文正文',
'Tagrove workspace with images, documents and a tag index':'拾签中文工作区：图片、文档与标签索引',
'Floating tag palette with categorized tags and drag handles':'中文标签浮窗，包含标签分类与拖动手柄',
'Search results showing which tags matched the query':'中文搜索结果与命中标签依据',
'Illustration of the browser capture workflow':'网页采集流程示意',
'PDF reader in the inspector showing continuous pages and zoom controls':'中文 PDF 阅读器，展示连续页面与缩放控件',
'Twelve accent colors; ultramarine is the default':'12 种主题色，默认为群青',
'Actual Tagrove workspace in dark mode':'拾签真实中文界面的深色工作区',
'Ultramarine (default)':'群青（默认）','Leaf':'荷叶绿','Violet':'紫色','Rose':'玫瑰','Amber':'琥珀','Graphite':'石墨','Teal':'青色','Terracotta':'陶土','Indigo':'靛蓝','Vermilion':'朱红','Moss':'苔绿','Cocoa':'可可',
'Play the workspace demo':'播放工作区演示','Play the tag palette demo':'播放标签浮窗演示','Play the search demo':'播放搜索演示','Play the PDF reader demo':'播放 PDF 阅读器演示',
}
for en,zh in [('Riverside tower.jpg','江畔塔楼-黄昏.jpg'),('Concrete facade.jpg','混凝土立面.jpg'),('Concept draft.pdf','方案文本-初稿.pdf'),('Material palette.jpg','外墙材料样板.jpg')]:
 attributes['Apply selected tag to '+en]='将选中标签贴到「'+zh+'」'
messages={
**{k:v for k,v in copy.items() if '\n'not in k},
' selected · ':'已选 · ',
' matching references. Click any card to add this tag.':' 份匹配资料。点击任意卡片贴上这个标签。',
'Choose a tag first, then click a reference.':'请先选一枚标签，再点击资料卡片。',
'This reference already has the ':'这份资料已经有「',' tag.':'」标签。',
'Added ':'已贴上「','. Original file stays in place.':'」，原件仍留在原处。',
'References grouped by their first tag. Original files stay in place.':'已按首枚标签分组，原件仍留在原处。',
'Original reference order restored.':'已恢复原来的资料顺序。',
'Demo reset. Try another tag.':'演示已重置，换一枚标签试试。',
'Loading…':'加载中…','Stop demo':'停止演示',
}
class Localize(HTMLParser):
 def __init__(self):
  super().__init__(convert_charrefs=True);self.out=[];self.raw=None;self.langlink=False;self.missing=[]
 def handle_starttag(self,tag,attrs):
  a=dict(attrs);raw=self.get_starttag_text()
  if tag in ['script','style']:self.raw=tag
  if tag=='html':raw=raw.replace('lang="en"','lang="zh-CN"')
  self.langlink=tag=='a'and a.get('class')=='language-switch'
  if self.langlink:
   raw=raw.replace('href="zh.html"','href="index.html"').replace('hreflang="zh-CN"','hreflang="en"').replace('lang="zh-CN"','lang="en"')
  for k,v in attrs:
   if k in ['src','data-src','content'] and v and 'assets/ui/' in v:
    raw=raw.replace(v,v.replace('-en.','-zh.'))
   if k=='href' and v and v.startswith('https://tagrove.pages.dev/') and a.get('rel')=='canonical':
    raw=raw.replace('"'+v+'"','"https://tagrove.pages.dev/zh.html"')
   if k in ['alt','aria-label','title','content'] and v:
    translated=attributes.get(v,copy.get(v))
    if translated is not None:raw=raw.replace('"'+html.escape(v,quote=True)+'"','"'+html.escape(translated,quote=True)+'"')
  self.out.append(raw)
 def handle_startendtag(self,tag,attrs):self.handle_starttag(tag,attrs)
 def handle_endtag(self,tag):
  self.out.append('</'+tag+'>')
  if tag in ['style','script']:self.raw=None
  if tag=='a':self.langlink=False
 def handle_data(self,data):
  if self.raw=='style':self.out.append(data);return
  if self.raw=='script':
   data=re.sub(r'"(?:[^"\\]|\\.)*"',lambda m:json.dumps(messages.get(json.loads(m[0]),json.loads(m[0])),ensure_ascii=False),data)
   self.out.append(data);return
  normalized=' '.join(data.split())
  if self.langlink and normalized=='中文':self.out.append('English');return
  if normalized in copy:
   lead=data[:len(data)-len(data.lstrip())];tail=data[len(data.rstrip()):]
   self.out.append(lead+html.escape(copy[normalized],quote=False)+tail)
  else:
   if re.search('[A-Za-z]',normalized):self.missing.append(normalized)
   self.out.append(data)
 def handle_decl(self,data):self.out.append('<!'+data+'>')
 def handle_comment(self,data):self.out.append('<!--'+data+'-->')
 def handle_entityref(self,name):self.out.append('&'+name+';')
 def handle_charref(self,name):self.out.append('&#'+name+';')
source=(site/'index.html').read_text(encoding='utf-8')
p=Localize();p.feed(source)
assert not p.missing,p.missing
result=''.join(p.out)
result=re.sub(r'<h1 id="h1">[\s\S]*?</h1>','<h1 id="h1">顺手贴上标签，<br /><em>随时找回资料。</em></h1>',result,count=1)
assert '-en.mp4'not in result and '-en.webp'not in result
(site/'zh.html').write_text(result,encoding='utf-8')
print('Built zh.html: Chinese copy, accessible labels, demo messages and Chinese media references.')

