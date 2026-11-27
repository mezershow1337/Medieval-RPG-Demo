#!/usr/bin/env python3
"""Страницы рукописей городской библиотеки (свои, процедурные): раскрытые кодексы и свитки на
пюпитрах, вывеска над входом и мозаичная надпись в торце читального зала.

Пишет:
  assets/textures/library/codices_a.jpg   2048×2048 — разворот 0..5 (по 1024×682, 2 столбца × 3 ряда)
  assets/textures/library/codices_b.jpg   2048×2048 — развороты 6..11
  assets/textures/library/inscriptions.jpg 2048×1024 — мозаика (верх, 2048×576) и вывеска (низ, 2048×448)

Номер разворота = номер рукописи в src/library_texts.rs (0 — пурпурное Евангелие, 5–6 — свитки
на папирусе, 9 — недописанный лист переписчика). Греческие тексты — подлинные начала
(общественное достояние), письмо — унциал (библейское маюскульное) без пробелов и ударений, как
в рукописях VI века. Буквы рисуются шрифтом Alegreya (OFL, лежит в assets/fonts), лунная сигма —
латинской C, эпсилон — Є, омега — ω. Пергамент, папирус, пурпур, чернила, киноварь, золото —
шумом и мазками, без чужих картинок.

Запуск:  python3 tools/textures/build_library_textures.py
Нужны Pillow и numpy.
"""
import os, random, unicodedata
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "assets", "textures", "library")
FONT = os.path.join(ROOT, "assets", "fonts", "Alegreya-Bold.ttf")
FONT_R = os.path.join(ROOT, "assets", "fonts", "Alegreya-Regular.ttf")

CELL = (1024, 682)
SS = 2  # рисуем вдвое крупнее и уменьшаем — сглаживание


# ---------------------------------------------------------------- тексты

JOHN = ("Ἐν ἀρχῇ ἦν ὁ λόγος, καὶ ὁ λόγος ἦν πρὸς τὸν θεόν, καὶ θεὸς ἦν ὁ λόγος. οὗτος ἦν ἐν ἀρχῇ πρὸς τὸν θεόν. "
        "πάντα δι᾽ αὐτοῦ ἐγένετο, καὶ χωρὶς αὐτοῦ ἐγένετο οὐδὲ ἕν ὃ γέγονεν. ἐν αὐτῷ ζωὴ ἦν, καὶ ἡ ζωὴ ἦν τὸ φῶς τῶν ἀνθρώπων. "
        "καὶ τὸ φῶς ἐν τῇ σκοτίᾳ φαίνει, καὶ ἡ σκοτία αὐτὸ οὐ κατέλαβεν. ἐγένετο ἄνθρωπος ἀπεσταλμένος παρὰ θεοῦ, ὄνομα αὐτῷ Ἰωάννης. "
        "οὗτος ἦλθεν εἰς μαρτυρίαν, ἵνα μαρτυρήσῃ περὶ τοῦ φωτός, ἵνα πάντες πιστεύσωσιν δι᾽ αὐτοῦ. οὐκ ἦν ἐκεῖνος τὸ φῶς, "
        "ἀλλ᾽ ἵνα μαρτυρήσῃ περὶ τοῦ φωτός. ἦν τὸ φῶς τὸ ἀληθινόν, ὃ φωτίζει πάντα ἄνθρωπον, ἐρχόμενον εἰς τὸν κόσμον.")
STRABO = ("Ἐν δὲ τῇ λοιπῇ Χερρονήσῳ πόλις ἐστὶν Ἡρακλεωτῶν, ἄποικος τῶν ἐν τῷ Πόντῳ, αὐτὸ τοῦτο Χερρόνησος καλουμένη, "
          "διέχουσα τοῦ Τύρα παράπλουν σταδίους τετρακισχιλίους τετρακοσίους. ἐν ᾗ τὸ τῆς Παρθένου ἱερόν, δαίμονός τινος, "
          "ἧς ἐπώνυμος καὶ ἡ ἄκρα ἡ πρὸ τῆς πόλεώς ἐστιν ἐν σταδίοις ἑκατόν, καλουμένη Παρθένιον, ἔχον νεὼν τῆς δαίμονος καὶ ξόανον. "
          "μεταξὺ δὲ τῆς πόλεως καὶ τῆς ἄκρας λιμένες τρεῖς. εἶθ᾽ ἡ παλαιὰ Χερρόνησος κατεσκαμμένη, καὶ μετ᾽ αὐτὴν λιμὴν στενόστομος.")
HERODOTUS = ("Ταύροισι δὲ νόμοισι τοιοῖσιδε χρέωνται· θύουσι μὲν τῇ Παρθένῳ τούς τε ναυηγοὺς καὶ τοὺς ἂν λάβωσι Ἑλλήνων "
             "ἐπαναχθέντες τρόπῳ τοιῷδε· καταρξάμενοι ῥοπάλῳ παίουσι τὴν κεφαλήν. οἱ μὲν δὴ λέγουσι ὡς τὸ σῶμα ἀπὸ τοῦ κρημνοῦ "
             "ὠθέουσι κάτω· ἐπὶ γὰρ κρημνοῦ ἵδρυται τὸ ἱρόν· τὴν δὲ κεφαλὴν ἀνασταυροῦσι. τὴν δὲ δαίμονα ταύτην τῇ θύουσι λέγουσι "
             "αὐτοὶ Ταῦροι Ἰφιγένειαν τὴν Ἀγαμέμνονος εἶναι.")
EURIPIDES = ("Πέλοψ ὁ Ταντάλειος ἐς Πῖσαν μολὼν θοαῖσιν ἵπποις Οἰνομάου γαμεῖ κόρην, ἐξ ἧς Ἀτρεὺς ἔβλαστεν· Ἀτρέως δὲ παῖς "
             "Μενέλαος Ἀγαμέμνων τε· τοῦ δ᾽ ἔφυν ἐγώ, τῆς Τυνδαρείας θυγατρὸς Ἰφιγένεια παῖς, ἣν ἀμφὶ δίνας ἃς θάμ᾽ Εὔριπος πυκναῖς "
             "αὔραις ἑλίσσων κυανέαν ἅλα στρέφει, ἔσφαξεν Ἑλένης οὕνεχ᾽, ὡς δοκεῖ, πατὴρ Ἀρτέμιδι κλειναῖς ἐν πτυχαῖσιν Αὐλίδος.")
EUCLID = ("Σημεῖόν ἐστιν, οὗ μέρος οὐθέν. Γραμμὴ δὲ μῆκος ἀπλατές. Γραμμῆς δὲ πέρατα σημεῖα. Εὐθεῖα γραμμή ἐστιν, ἥτις ἐξ ἴσου "
          "τοῖς ἐφ᾽ ἑαυτῆς σημείοις κεῖται. Ἐπιφάνεια δέ ἐστιν, ὃ μῆκος καὶ πλάτος μόνον ἔχει. Ἐπιφανείας δὲ πέρατα γραμμαί. "
          "Ἐπίπεδος ἐπιφάνειά ἐστιν, ἥτις ἐξ ἴσου ταῖς ἐφ᾽ ἑαυτῆς εὐθείαις κεῖται. Κύκλος ἐστὶ σχῆμα ἐπίπεδον ὑπὸ μιᾶς γραμμῆς "
          "περιεχόμενον, πρὸς ἣν ἀφ᾽ ἑνὸς σημείου τῶν ἐντὸς τοῦ σχήματος κειμένων πᾶσαι αἱ προσπίπτουσαι εὐθεῖαι ἴσαι ἀλλήλαις εἰσίν.")
ILIAD = ["Μῆνιν ἄειδε θεὰ Πηληϊάδεω Ἀχιλῆος", "οὐλομένην, ἣ μυρί᾽ Ἀχαιοῖς ἄλγε᾽ ἔθηκε,", "πολλὰς δ᾽ ἰφθίμους ψυχὰς Ἄϊδι προΐαψεν",
         "ἡρώων, αὐτοὺς δὲ ἑλώρια τεῦχε κύνεσσιν", "οἰωνοῖσί τε πᾶσι, Διὸς δ᾽ ἐτελείετο βουλή,", "ἐξ οὗ δὴ τὰ πρῶτα διαστήτην ἐρίσαντε",
         "Ἀτρεΐδης τε ἄναξ ἀνδρῶν καὶ δῖος Ἀχιλλεύς.", "τίς τ᾽ ἄρ σφωε θεῶν ἔριδι ξυνέηκε μάχεσθαι;", "Λητοῦς καὶ Διὸς υἱός· ὃ γὰρ βασιλῆϊ χολωθεὶς",
         "νοῦσον ἀνὰ στρατὸν ὄρσε κακήν, ὀλέκοντο δὲ λαοί,", "οὕνεκα τὸν Χρύσην ἠτίμασεν ἀρητῆρα", "Ἀτρεΐδης· ὃ γὰρ ἦλθε θοὰς ἐπὶ νῆας Ἀχαιῶν",
         "λυσόμενός τε θύγατρα φέρων τ᾽ ἀπερείσι᾽ ἄποινα,", "στέμματ᾽ ἔχων ἐν χερσὶν ἑκηβόλου Ἀπόλλωνος", "χρυσέῳ ἀνὰ σκήπτρῳ, καὶ λίσσετο πάντας Ἀχαιούς,",
         "Ἀτρεΐδα δὲ μάλιστα δύω, κοσμήτορε λαῶν·"]
OATH = ("Ὀμνύω Δία, Γᾶν, Ἅλιον, Παρθένον, θεοὺς Ὀλυμπίους καὶ Ὀλυμπίας καὶ ἥρωας ὅσοι πόλιν καὶ χώραν καὶ τείχη ἔχοντι τὰ Χερσονασιτᾶν. "
        "ὁμονοησῶ ὑπὲρ σωτηρίας καὶ ἐλευθερίας πόλεος καὶ πολιτᾶν καὶ οὐ προδωσῶ Χερσόνασον οὐδὲ Κερκινῖτιν οὐδὲ Καλὸν λιμένα "
        "οὐδὲ τἆλλα τείχη οὐδὲ τᾶς ἄλλας χώρας ἇς Χερσονασῖται νέμονται ἢ ἐνέμοντο οὐδὲν οὐθενὶ οὔτε Ἕλλανι οὔτε βαρβάρῳ.")
PSALM = ("Μακάριος ἀνήρ, ὃς οὐκ ἐπορεύθη ἐν βουλῇ ἀσεβῶν καὶ ἐν ὁδῷ ἁμαρτωλῶν οὐκ ἔστη καὶ ἐπὶ καθέδραν λοιμῶν οὐκ ἐκάθισεν, "
         "ἀλλ᾽ ἢ ἐν τῷ νόμῳ κυρίου τὸ θέλημα αὐτοῦ, καὶ ἐν τῷ νόμῳ αὐτοῦ μελετήσει ἡμέρας καὶ νυκτός. καὶ ἔσται ὡς τὸ ξύλον "
         "τὸ πεφυτευμένον παρὰ τὰς διεξόδους τῶν ὑδάτων, ὃ τὸν καρπὸν αὐτοῦ δώσει ἐν καιρῷ αὐτοῦ, καὶ τὸ φύλλον αὐτοῦ οὐκ ἀπορρυήσεται.")
PINAX = [
    "ΠΙΝΑΞ ΤΩΝ ΒΙΒΛΙΩΝ", "Α΄ ΕΥΑΓΓΕΛΙΟΝ ΤΕΤΡΑ", "Β΄ ΑΠΟΣΤΟΛΟΣ", "Γ΄ ΨΑΛΤΗΡΙΑ Δ΄", "Δ΄ ΠΡΟΦΗΤΑΙ", "Ε΄ ΒΑΣΙΛΕΙΟΥ ΕΞΑΗΜΕΡΟΣ",
    "Ϛ΄ ΓΡΗΓΟΡΙΟΥ ΛΟΓΟΙ", "Ζ΄ ΙΩΑΝΝΟΥ ΟΜΙΛΙΑΙ", "Η΄ ΗΡΟΔΟΤΟΥ ΙΣΤΟΡΙΑΙ", "Θ΄ ΘΟΥΚΥΔΙΔΗΣ", "Ι΄ ΕΥΣΕΒΙΟΥ ΕΚΚΛ ΙΣΤ",
    "ΙΑ΄ ΣΤΡΑΒΩΝ", "ΙΒ΄ ΙΠΠΟΚΡΑΤΗΣ", "ΙΓ΄ ΓΑΛΗΝΟΣ", "ΙΔ΄ ΔΙΟΣΚΟΥΡΙΔΗΣ", "ΙΕ΄ ΟΜΗΡΟΣ", "ΙϚ΄ ΕΥΡΙΠΙΔΗΣ", "ΙΖ΄ ΜΕΝΑΝΔΡΟΣ",
    "ΙΗ΄ ΑΙΣΩΠΟΣ", "ΙΘ΄ ΟΡΚΟΣ ΠΟΛΙΤΩΝ", "Κ΄ ΕΥΚΛΕΙΔΗΣ", "ΚΑ΄ ΝΙΚΟΜΑΧΟΣ", "ΚΒ΄ ΠΤΟΛΕΜΑΙΟΣ",
]

NOMINA = {"ΘΕΟΝ": "ΘΝ", "ΘΕΟΥ": "ΘΥ", "ΘΕΟΣ": "ΘΣ", "ΚΥΡΙΟΥ": "ΚΥ", "ΙΗΣΟΥΣ": "ΙΣ", "ΧΡΙΣΤΟΣ": "ΧΣ", "ΑΝΘΡΩΠΟΝ": "ΑΝΟΝ", "ΑΝΘΡΩΠΩΝ": "ΑΝΩΝ"}


def uncial(s, nomina=False):
    """Унциал: без ударений и придыханий, заглавными, сплошным письмом; сокращения имён Бога."""
    s = unicodedata.normalize("NFD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.replace("᾽", "").replace("’", "").replace("ʼ", "").upper()
    words = []
    for w in s.replace("·", " · ").split():
        core = "".join(c for c in w if c.isalpha())
        if nomina and core in NOMINA:
            words.append("¦" + NOMINA[core] + "¦" + w[len(core):])
        else:
            words.append(w)
    out = []
    for w in words:
        if w in (",", "·", ";"):
            continue
        out.append(w.replace(",", "").replace(";", "").replace(".", "·"))
    return "".join(out)


def glyph(c):
    return {"Σ": "C", "Ε": "Є", "Ω": "ω", "Ϲ": "C"}.get(c, c)


# ---------------------------------------------------------------- шум и поверхности

def fbm(w, h, seed, octaves=5, base=4):
    rng = np.random.default_rng(seed)
    acc = np.zeros((h, w), np.float32)
    amp, tot = 1.0, 0.0
    for o in range(octaves):
        n = base * 2 ** o
        small = rng.random((n + 1, int(n * w / h) + 1)).astype(np.float32)
        img = Image.fromarray((small * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC)
        acc += np.asarray(img, np.float32) / 255.0 * amp
        tot += amp
        amp *= 0.55
    return acc / tot


def surface(w, h, kind, seed):
    """Подложка: 'parchment', 'purple', 'papyrus'. Возвращает float RGB 0..1."""
    rng = np.random.default_rng(seed)
    n1 = fbm(w, h, seed, 6, 3)
    n2 = fbm(w, h, seed + 7, 4, 12)
    if kind == "purple":
        base = np.array([0.36, 0.11, 0.25])
        tint = np.array([0.22, 0.06, 0.17])
        col = base[None, None] * (0.75 + 0.5 * n1[..., None]) + (tint - base)[None, None] * (n2[..., None] - 0.5) * 0.6
    elif kind == "papyrus":
        base = np.array([0.80, 0.68, 0.45])
        col = base[None, None] * (0.86 + 0.24 * n1[..., None])
        # Волокна: горизонтальные (лицевой слой) и редкие вертикальные под ними.
        fib = rng.random((h, 1)).astype(np.float32)
        fib = np.asarray(Image.fromarray((fib * 255).astype(np.uint8)).resize((w // 8, h), Image.BILINEAR).resize((w, h), Image.BICUBIC), np.float32) / 255
        fv = rng.random((1, w)).astype(np.float32)
        fv = np.asarray(Image.fromarray((fv * 255).astype(np.uint8)).resize((w, h // 8), Image.BILINEAR).resize((w, h), Image.BICUBIC), np.float32) / 255
        col *= (0.9 + 0.16 * fib[..., None]) * (0.96 + 0.06 * fv[..., None])
        col *= np.array([1.0, 0.98, 0.93])[None, None]
    else:
        base = np.array([0.89, 0.81, 0.64])
        col = base[None, None] * (0.88 + 0.2 * n1[..., None])
        # Мездра и волосяная сторона: мелкие тёмные точки фолликулов.
        dots = (rng.random((h, w)) > 0.9965).astype(np.float32)
        dots = np.asarray(Image.fromarray((dots * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.2)), np.float32) / 255
        col *= (1.0 - 0.5 * dots[..., None])
        col *= (0.97 + 0.06 * n2[..., None])
    # Пятна и потемнение к краям.
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    ex = np.minimum(xx, w - 1 - xx) / (w * 0.5)
    ey = np.minimum(yy, h - 1 - yy) / (h * 0.5)
    edge = np.clip(np.minimum(ex, ey) * 6.0, 0, 1)
    col *= (0.72 + 0.28 * edge[..., None])
    stains = np.clip((fbm(w, h, seed + 99, 3, 2) - 0.62) * 4.0, 0, 1)
    col *= (1.0 - 0.18 * stains[..., None]) * np.array([1.0, 0.97, 0.9])[None, None] ** stains[..., None]
    return np.clip(col, 0, 1)


def gutter(col, x0, x1, h):
    """Тень в корешке разворота и выпуклость листов."""
    w = col.shape[1]
    xs = np.arange(w, dtype=np.float32)
    mid = (x0 + x1) / 2.0
    d = np.abs(xs - mid) / ((x1 - x0) / 2.0)
    shade = 0.55 + 0.45 * np.clip(d * 3.0, 0, 1) ** 0.6
    bulge = 1.0 + 0.05 * np.sin(np.clip(d, 0, 1) * np.pi)
    col *= (shade * bulge)[None, :, None]
    return col


def to_img(col):
    return Image.fromarray((np.clip(col, 0, 1) * 255).astype(np.uint8), "RGB")


# ---------------------------------------------------------------- письмо

class Pen:
    def __init__(self, img, size, ink, seed, font=FONT):
        self.d = ImageDraw.Draw(img, "RGBA")
        self.img = img
        self.font = ImageFont.truetype(font, size)
        self.size = size
        self.ink = ink
        self.rng = random.Random(seed)

    def width(self, c):
        return self.font.getlength(glyph(c))

    def char(self, x, y, c, color=None, scale=1.0):
        g = glyph(c)
        col = color or self.ink
        a = self.rng.randint(200, 255)
        f = self.font if scale == 1.0 else ImageFont.truetype(self.font.path, int(self.size * scale))
        jx, jy = self.rng.uniform(-0.6, 0.6), self.rng.uniform(-0.8, 0.8)
        self.d.text((x + jx, y + jy), g, font=f, fill=col + (a,))
        return f.getlength(g)

    def block(self, text, x0, y0, w, lines, lh, rubric=None, initial=True, overline_col=None):
        """Столбец сплошного письма; ¦…¦ — сокращение имени (над чертой, `overline_col`)."""
        i = 0
        y = y0
        first = True
        sacred = False
        for ln in range(lines):
            x = x0
            # Буквица на выступе (эктесис) в начале абзаца.
            if first and initial and i < len(text):
                c = text[i]
                adv = self.char(x - self.size * 0.55, y - self.size * 0.35, c, rubric or self.ink, 1.7)
                x += adv - self.size * 0.55 + 2
                i += 1
                first = False
            while i < len(text):
                c = text[i]
                if c == "¦":
                    sacred = not sacred
                    i += 1
                    continue
                cw = self.width(c)
                if x + cw > x0 + w:
                    break
                col = overline_col if (sacred and overline_col) else None
                adv = self.char(x, y, c, col)
                if sacred and overline_col:
                    self.d.line([(x - 1, y + self.size * 0.12), (x + adv + 1, y + self.size * 0.12)], fill=overline_col + (230,), width=max(2, self.size // 14))
                x += adv + self.size * 0.02
                i += 1
            y += lh
            if i >= len(text):
                i = 0  # текст по кругу: важен вид страницы
        return y

    def ruling(self, x0, y0, w, lines, lh, color=(120, 95, 60)):
        for k in range(lines):
            y = y0 + k * lh + self.size * 0.95
            self.d.line([(x0 - 6, y), (x0 + w + 6, y)], fill=color + (38,), width=2)
        for x in (x0 - 6, x0 + w + 6):
            self.d.line([(x, y0 - lh * 0.3), (x, y0 + lines * lh)], fill=color + (38,), width=2)


def headpiece(d, x0, y, w, rng, colors):
    """Заставка: плетёнка из двух лент и киноварные точки."""
    h = 26
    d.rectangle([x0, y, x0 + w, y + h], outline=colors[0] + (220,), width=3)
    step = 22
    for k, x in enumerate(range(int(x0) + 4, int(x0 + w) - 10, step)):
        c = colors[k % len(colors)]
        d.arc([x, y + 4, x + step, y + h - 4], 0, 180, fill=c + (230,), width=4)
        d.arc([x, y + 4, x + step, y + h - 4], 180, 360, fill=colors[(k + 1) % len(colors)] + (230,), width=4)
    for x in (x0 - 14, x0 + w + 14):
        d.ellipse([x - 5, y + h / 2 - 5, x + 5, y + h / 2 + 5], fill=colors[0] + (230,))


def coronis(d, x, y, col):
    d.line([(x, y), (x + 40, y)], fill=col + (220,), width=3)
    d.polygon([(x + 40, y - 7), (x + 52, y), (x + 40, y + 7)], fill=col + (220,))


# ---------------------------------------------------------------- развороты

INK = (52, 34, 22)
MINIUM = (178, 52, 28)
BLUE = (40, 70, 130)
GOLD = (214, 172, 82)
SILVER = (214, 214, 222)


def spread(kind, seed, draw_pages):
    W, H = CELL[0] * SS, CELL[1] * SS
    col = surface(W, H, kind, seed)
    if kind != "papyrus":
        col = gutter(col, 0, W, H)
    img = to_img(col)
    draw_pages(img, W, H, random.Random(seed))
    return img.resize(CELL, Image.LANCZOS)


def codex_pages(text, *, cols=2, size=34, lines=None, ink=INK, rubric=MINIUM, kind="parchment", title=None, nomina=False, extra=None):
    t = uncial(text, nomina)

    def draw(img, W, H, rng):
        page_w = W / 2
        mx, mt, mb = page_w * 0.13, H * 0.12, H * 0.16
        gap = page_w * 0.05
        lh = size * 1.32
        n = lines or int((H - mt - mb) / lh)
        pos = 0
        for p in range(2):
            x0 = p * page_w + (mx if p == 1 else page_w * 0.11)
            inner = page_w - mx - page_w * 0.11
            cw = (inner - gap * (cols - 1)) / cols
            pen = Pen(img, size, ink, rng.randint(0, 1 << 30))
            y0 = mt
            if title and p == 0:
                headpiece(pen.d, x0, y0 - size * 1.2, inner, rng, [rubric, BLUE, GOLD] if kind != "purple" else [GOLD, SILVER])
                tp = Pen(img, int(size * 0.9), rubric, 3)
                tx = x0 + inner / 2 - sum(tp.width(c) for c in title) / 2
                for c in title:
                    tx += tp.char(tx, y0, c) + 2
                y0 += lh * 1.4
            for c in range(cols):
                cx = x0 + c * (cw + gap)
                pen.ruling(cx, y0, cw, n - (2 if title and p == 0 else 0), lh)
                pen.block(t[pos:] + t, cx, y0, cw, n - (2 if title and p == 0 else 0), lh, rubric=rubric, initial=(c == 0 and p == 0) or rng.random() < 0.35,
                          overline_col=GOLD if nomina else None)
                pos = (pos + 40 * n) % max(1, len(t))
            # Номер листа и колонтитул.
            fol = Pen(img, int(size * 0.7), rubric, 9)
            fol.char(x0 + inner - 30, H - mb * 0.55, "ΙΖ"[p])
            if extra:
                extra(pen, p, x0, inner, H, rng)
    return draw


def scroll_columns(lines, size=32, ink=(40, 30, 22)):
    def draw(img, W, H, rng):
        pen = Pen(img, size, ink, rng.randint(0, 99999))
        lh = size * 1.25
        n = int(H * 0.78 / lh)
        colw = W * 0.4
        # Склейки листов (коллемы) — тонкие вертикальные полосы.
        for x in range(int(W * 0.07), W, int(W * 0.31)):
            pen.d.line([(x, 0), (x, H)], fill=(90, 60, 30, 46), width=5)
        k = 0
        for c in range(2):
            x0 = W * 0.06 + c * (colw + W * 0.1)
            y = H * 0.11
            for _ in range(n):
                t = uncial(lines[k % len(lines)])
                x = x0
                for ch in t:
                    cw = pen.width(ch)
                    if x + cw > x0 + colw * 1.12:
                        break
                    x += pen.char(x, y, ch) + 1
                y += lh
                k += 1
        # Обтрёпанные края.
        for y in range(0, H, 6):
            r = rng.randint(0, 14)
            pen.d.rectangle([0, y, r, y + 6], fill=(60, 45, 30, 120))
            r = rng.randint(0, 14)
            pen.d.rectangle([W - r, y, W, y + 6], fill=(60, 45, 30, 120))
    return draw


def pinax_pages():
    def draw(img, W, H, rng):
        page_w = W / 2
        for p in range(2):
            pen = Pen(img, 30, INK, 40 + p)
            x0 = p * page_w + page_w * 0.12
            y = H * 0.1
            items = PINAX if p == 0 else PINAX[11:] + PINAX[1:6]
            for k, s in enumerate(items):
                col = MINIUM if (k == 0 and p == 0) else None
                x = x0
                for ch in uncial(s):
                    x += pen.char(x, y, ch, col) + 2
                    if ch == "΄":
                        x += 8
                # Пометки на полях и вычеркнутые строки.
                if rng.random() < 0.18:
                    pen.d.line([(x0, y + 22), (x, y + 18)], fill=INK + (180,), width=3)
                if rng.random() < 0.2:
                    m = Pen(img, 20, (90, 60, 40), k)
                    mx = x + 20
                    for ch in uncial("ΑΠΕΔΟΘΗ"):
                        mx += m.char(mx, y + 6, ch) + 1
                y += 30 * 1.45
                if y > H * 0.86:
                    break
            # Клякса.
            if p == 1:
                cx, cy = x0 + page_w * 0.55, H * 0.7
                for _ in range(9):
                    r = rng.uniform(6, 22)
                    pen.d.ellipse([cx - r + rng.uniform(-18, 18), cy - r + rng.uniform(-12, 12), cx + r, cy + r], fill=INK + (200,))
    return draw


def scribe_sheet():
    """Недописанный лист Феодора: разлиновано, написано полстраницы, дальше чисто."""
    t = uncial(PSALM)

    def draw(img, W, H, rng):
        page_w = W / 2
        pen = Pen(img, 34, INK, 77)
        lh = 34 * 1.32
        for p in range(2):
            x0 = p * page_w + page_w * 0.12
            n = int(H * 0.72 / lh)
            pen.ruling(x0, H * 0.12, page_w * 0.76, n, lh)
            # Наколы по краю для линовки.
            for k in range(n):
                pen.d.ellipse([x0 + page_w * 0.8, H * 0.12 + k * lh + 30, x0 + page_w * 0.8 + 4, H * 0.12 + k * lh + 34], fill=(60, 40, 30, 150))
            if p == 0:
                pen.block(t, x0, H * 0.12, page_w * 0.76, n, lh, rubric=MINIUM)
            else:
                pen.block(t[len(t) // 2:], x0, H * 0.12, page_w * 0.76, n // 2, lh, rubric=MINIUM, initial=False)
                # Перо кончилось на полуслове: брызги и пробные штрихи на полях.
                for _ in range(5):
                    x, y = x0 + rng.uniform(0, page_w * 0.7), H * 0.12 + (n // 2 + 1) * lh + rng.uniform(0, 40)
                    pen.d.line([(x, y), (x + rng.uniform(10, 40), y + rng.uniform(-6, 6))], fill=INK + (160,), width=3)
    return draw


def purple_extra(pen, p, x0, inner, H, rng):
    # Евсевиевы каноны под столбцами: золотые арочки.
    y = H * 0.87
    for k in range(4):
        x = x0 + k * inner / 4 + 10
        pen.d.arc([x, y, x + inner / 4 - 20, y + 60], 180, 360, fill=GOLD + (230,), width=4)


def build_spreads():
    S = []
    S.append(spread("purple", 1, codex_pages(JOHN, cols=2, size=36, ink=SILVER, rubric=GOLD, kind="purple", title=uncial("ΚΑΤΑ ΙΩΑΝΝΗΝ"), nomina=True, extra=purple_extra)))
    S.append(spread("parchment", 2, codex_pages(STRABO, cols=1, size=30, title=uncial("ΣΤΡΑΒΩΝΟΣ Ζ"))))
    S.append(spread("parchment", 3, codex_pages(HERODOTUS, cols=1, size=32, title=uncial("ΜΕΛΠΟΜΕΝΗ"))))
    S.append(spread("parchment", 4, codex_pages(EURIPIDES, cols=1, size=36, title=uncial("ΙΦΙΓΕΝΕΙΑ"))))

    def euclid_extra(pen, p, x0, inner, H, rng):
        # Чертёж киноварью на полях: круг и треугольник.
        cx = x0 + inner + 70 if p == 1 else x0 - 70
        cy = H * 0.45
        pen.d.ellipse([cx - 52, cy - 52, cx + 52, cy + 52], outline=MINIUM + (220,), width=3)
        pen.d.polygon([(cx - 45, cy + 26), (cx + 45, cy + 26), (cx, cy - 52)], outline=MINIUM + (220,))
        pen.d.line([(cx, cy), (cx + 52, cy)], fill=MINIUM + (220,), width=2)

    S.append(spread("parchment", 5, codex_pages(EUCLID, cols=1, size=30, title=uncial("ΕΥΚΛΕΙΔΟΥ ΣΤΟΙΧΕΙΑ"), extra=euclid_extra)))
    S.append(spread("papyrus", 6, scroll_columns(ILIAD)))
    oath_lines = [OATH[i:i + 34] for i in range(0, len(OATH), 34)]
    S.append(spread("papyrus", 7, scroll_columns(oath_lines, size=30, ink=(52, 36, 24))))
    S.append(spread("parchment", 8, codex_pages(PSALM, cols=1, size=36, title=uncial("ΨΑΛΜΟΣ Α"))))
    S.append(spread("parchment", 9, pinax_pages()))
    S.append(spread("parchment", 10, scribe_sheet()))
    # Запасные развороты (листы на столах): начало Евангелия и Псалтири простым пергаментом.
    S.append(spread("parchment", 11, codex_pages(JOHN, cols=2, size=30, nomina=True)))
    S.append(spread("parchment", 12, codex_pages(PSALM, cols=2, size=30)))
    return S


def atlas(spreads):
    out = []
    for a in range(2):
        img = Image.new("RGB", (2048, 2048), (40, 30, 20))
        for k in range(6):
            i = a * 6 + k
            if i < len(spreads):
                img.paste(spreads[i], ((k % 2) * 1024, (k // 2) * 682))
        out.append(img)
    return out


# ---------------------------------------------------------------- надписи

def mosaic(w, h, text, seed):
    """Мозаика: золотые смальты по синему, кайма-плетёнка, надпись белым мрамором."""
    rng = np.random.default_rng(seed)
    t = 14  # тессера (пиксели)
    img = Image.new("RGB", (w, h), (20, 20, 30))
    d = ImageDraw.Draw(img)
    # Маска надписи и каймы.
    mask = Image.new("L", (w, h), 0)
    md = ImageDraw.Draw(mask)
    # Две строки, по ширине поля между крестами.
    rows = text.split("|")
    size = int(h * 0.3)
    font = ImageFont.truetype(FONT, size)
    strs = ["".join(glyph(c) for c in uncial(r)) for r in rows]
    while max(font.getlength(s) for s in strs) > w * 0.72:
        size -= 4
        font = ImageFont.truetype(FONT, size)
    for k, s in enumerate(strs):
        tw = font.getlength(s)
        md.text(((w - tw) / 2, h * 0.12 + k * size * 1.12), s, font=font, fill=255)
    # Кресты по краям.
    for cx in (w * 0.09, w * 0.91):
        md.rectangle([cx - 16, h * 0.2, cx + 16, h * 0.8], fill=255)
        md.rectangle([cx - 60, h * 0.36, cx + 60, h * 0.47], fill=255)
    m = np.asarray(mask, np.float32) / 255
    for y in range(0, h, t):
        off = (y // t % 2) * t // 2
        for x in range(-off, w, t):
            cx, cy = min(w - 1, max(0, x + t // 2)), min(h - 1, y + t // 2)
            border = y < t * 3 or y > h - t * 4 or x < t * 3 or x > w - t * 4
            if border:
                k = ((x // t) + (y // t)) % 6
                base = [(200, 160, 70), (150, 30, 30), (200, 160, 70), (230, 225, 210), (30, 70, 60), (200, 160, 70)][k]
            elif m[cy, cx] > 0.5:
                g = rng.random()
                base = (222, 178, 84) if g < 0.8 else (245, 205, 110) if g < 0.95 else (170, 128, 52)
            else:
                g = rng.random()
                base = (28, 46, 98) if g < 0.7 else (40, 62, 120) if g < 0.93 else (18, 30, 70)
            j = rng.integers(-18, 18, 3)
            c = tuple(int(np.clip(base[i] + j[i], 0, 255)) for i in range(3))
            d.rectangle([x + 1, y + 1, x + t - 2 + rng.integers(-1, 2), y + t - 2 + rng.integers(-1, 2)], fill=c)
    return img


def sign(w, h, text, seed):
    """Мраморная доска с врезанной надписью, буквы заполнены киноварью."""
    n = fbm(w, h, seed + 3, 6, 2)
    warp = fbm(w, h, seed + 5, 4, 3)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    # Прожилки: тонкие изломанные линии вдоль диагонали, искажённые шумом.
    v = np.sin((xx * 0.004 + yy * 0.009) * 6.0 + warp * 9.0)
    veins = np.exp(-np.abs(v) * 14.0) * (0.4 + 0.6 * n)
    marble = np.array([0.91, 0.9, 0.87])[None, None] * (0.94 + 0.07 * n[..., None]) - veins[..., None] * np.array([0.22, 0.22, 0.2])
    img = to_img(marble)
    d = ImageDraw.Draw(img, "RGBA")
    d.rectangle([10, 10, w - 10, h - 10], outline=(120, 110, 100, 255), width=8)
    d.rectangle([34, 34, w - 34, h - 34], outline=(150, 140, 128, 255), width=3)
    font = ImageFont.truetype(FONT, int(h * 0.42))
    s = "".join(glyph(c) for c in uncial(text))
    tw = font.getlength(s)
    x, y = (w - tw) / 2, h * 0.22
    d.text((x + 3, y + 4), s, font=font, fill=(60, 50, 45, 200))
    d.text((x, y), s, font=font, fill=(150, 34, 24, 255))
    for cx in (70, w - 70):
        d.polygon([(cx, h / 2 - 40), (cx + 40, h / 2), (cx, h / 2 + 40), (cx - 40, h / 2)], fill=(150, 34, 24, 220))
    return img


def build_inscriptions():
    img = Image.new("RGB", (2048, 1024), (0, 0, 0))
    img.paste(mosaic(2048, 576, "ΑΡΧΗ ΣΟΦΙΑΣ|ΦΟΒΟΣ ΚΥΡΙΟΥ", 31), (0, 0))
    img.paste(sign(2048, 448, "ΒΙΒΛΙΟΘΗΚΗ", 32), (0, 576))
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    a, b = atlas(build_spreads())
    a.save(os.path.join(OUT, "codices_a.jpg"), quality=90)
    b.save(os.path.join(OUT, "codices_b.jpg"), quality=90)
    build_inscriptions().save(os.path.join(OUT, "inscriptions.jpg"), quality=90)
    print("готово:", OUT)


if __name__ == "__main__":
    main()
