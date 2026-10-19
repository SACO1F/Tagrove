"""Build the synthetic reference material used to capture real Tagrove UI
for the introduction page. Deterministic; safe to re-run."""
import os, shutil, struct
from PIL import Image

ROOT = os.path.join(os.path.dirname(__file__), "..", "qa", "v0-landing", "material")
QA = os.path.join(os.path.dirname(__file__), "..", "qa", "v0-landing")
V = os.path.join(os.path.dirname(__file__), "..", "vibe_images")
PLATES = {
    "plate-tower-dusk": ("江畔塔楼/参考", "江畔塔楼-黄昏-01.jpg"),
    "plate-facade-detail": ("江畔塔楼/参考", "立面细部-混凝土.jpg"),
    "plate-material-board": ("江畔塔楼/参考", "材料样板-外立面.jpg"),
    "plate-node-drawing": ("江畔塔楼/参考", "墙身节点-1比20.jpg"),
    "plate-interior-daylight": ("江畔塔楼/汇报", "大堂采光-室内参考.jpg"),
}
import glob

shutil.rmtree(QA, ignore_errors=True)


def path_for(sub, name):
    d = os.path.join(ROOT, sub)
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, name)


for stem, (sub, name) in PLATES.items():
    src = glob.glob(f"{V}/{stem}_*.png")[0]
    Image.open(src).convert("RGB").save(path_for(sub, name), "JPEG", quality=88, optimize=True)

path_for("江畔塔楼/参考", "幕墙分缝做法讨论.md").encode and open(
    path_for("江畔塔楼/参考", "幕墙分缝做法讨论.md"), "w", encoding="utf-8"
).write(
    "# 幕墙分缝做法讨论\n\n"
    "立面分缝与窗套节点需要与结构专业复核。当前方案采用 1200mm 模数分缝，"
    "窗套内衬 3mm 阳极氧化铝板，收口打胶宽度控制在 12mm。\n\n"
    "## 待确认\n\n"
    "- 分缝是否与室内吊顶对缝\n- 阳台栏板高度与分缝关系\n- 夜景照明预留槽位置\n"
)
open(path_for("江畔塔楼/汇报", "汇报大纲.md"), "w", encoding="utf-8").write(
    "# 江畔塔楼 方案汇报大纲\n\n"
    "1. 城市界面与塔楼体量\n2. 立面语言：清水混凝土与竖向遮阳\n"
    "3. 材料样板与造价区间\n4. 下一步：节点深化与幕墙招标\n"
)
open(path_for("江畔塔楼/成本", "成本估算表-外立面.csv"), "w", encoding="utf-8").write(
    "分项,单位,数量,单价,小计\n"
    "清水混凝土挂板,m2,1840,860,1582400\n"
    "竖向铝遮阳,m,920,340,312800\n"
    "Low-E 中空玻璃,m2,1260,720,907200\n"
    "窗套铝板,m,480,260,124800\n"
)

PAGES = [
    [
        "Tagrove sample document - facade study",
        "Section 3.2  Joint detailing and window reveal",
        "Board-formed concrete, 1200 mm module.",
        "Bronze anodised reveal, 3 mm lining.",
        "Sealant joint 12 mm nominal.",
        "",
        "Reference: tower facade competition board, sheet 14.",
        "(synthetic content used only for interface capture)",
    ],
    [
        "Section 3.3  Shading geometry",
        "Vertical fins, 450 mm depth, 1200 mm spacing.",
        "Daylight reflection study attached separately.",
        "Cost band: see the estimate sheet in this folder.",
        "",
        "Placeholder paragraph so the text layer can be selected.",
        "Placeholder paragraph so continuous reading is visible.",
        "Placeholder paragraph so page navigation is visible.",
    ],
]


def build_pdf(target, pages):
    objects = []

    def add(body):
        objects.append(body)
        return len(objects)

    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    contents = []
    for lines in pages:
        body = b"".join(
            b"BT /F1 11 Tf 56 %.0f Td (%s) Tj ET\n" % (760 - i * 22, l.encode("latin-1"))
            for i, l in enumerate(lines)
        )
        contents.append(add(b"<< /Length %d >>\nstream\n" % len(body) + body + b"endstream"))
    pages_id = len(objects) + 1 + len(contents)  # reserved below
    page_ids = []
    for cid in contents:
        page_ids.append(
            add(
                b"<< /Type /Page /Parent %d 0 R /Resources << /Font << /F1 %d >> >> "
                b"/MediaBox [0 0 595 842] /Contents %d 0 R >>" % (pages_id, font, cid)
            )
        )
    kids = b" ".join(b"%d 0 R" % i for i in page_ids)
    assert pages_id == add(b"<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(page_ids)))
    root = add(b"<< /Type /Catalog /Pages %d 0 R >>" % pages_id)

    out = b"%PDF-1.4\n"
    offsets = []
    for i, body in enumerate(objects):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % (i + 1) + body + b"endobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n" % (len(objects) + 1) + b"0000000000 65535 f \n"
    for o in offsets:
        out += b"%010d 00000 n \n" % o
    out += (
        b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n"
        % (len(objects) + 1, root, xref)
    )
    open(target, "wb").write(out)


build_pdf(path_for("江畔塔楼/参考", "方案文本-初稿.pdf"), PAGES)

for dp, _, files in os.walk(ROOT):
    for f in sorted(files):
        p = os.path.join(dp, f)
        print("%8.1f KB  %s" % (os.path.getsize(p) / 1024, p.replace("\\", "/")))
