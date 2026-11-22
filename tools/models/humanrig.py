"""Скелет игры (15 костей) поверх тела MakeHuman: позы привязки, веса, сжатые в кулак кисти.

Соглашение игры (src/rig.rs): в покое (все повороты единичные) руки и ноги висят прямо вниз,
ось -Y кости идёт вдоль конечности, -Z — «перёд» кости. Ладони смотрят к телу, стопы — вперёд.
Тело MakeHuman стоит в A-позе; разницу между A-позой и покоем игры хранят обратные матрицы
привязки (inverse bind matrices) скина: поворот кости в позе привязки R_b переводит её
игровую ось в настоящее направление кости.
"""

import numpy as np

BONES = ["pelvis", "torso", "head", "upper_arm_r", "forearm_r", "hand_r", "upper_arm_l", "forearm_l", "hand_l",
         "thigh_r", "shin_r", "foot_r", "thigh_l", "shin_l", "foot_l"]
PARENT = [None, 0, 1, 1, 3, 4, 1, 6, 7, 0, 9, 10, 0, 12, 13]
B = {n: i for i, n in enumerate(BONES)}


def norm(v):
    n = np.linalg.norm(v)
    return v / n if n > 1e-9 else v


def rot_axis(axis, ang):
    a = norm(np.asarray(axis, float))
    x, y, z = a
    c, s = np.cos(ang), np.sin(ang)
    C = 1 - c
    return np.array([[c + x * x * C, x * y * C - z * s, x * z * C + y * s],
                     [y * x * C + z * s, c + y * y * C, y * z * C - x * s],
                     [z * x * C - y * s, z * y * C + x * s, c + z * z * C]])


def rot_arc(a, b):
    a, b = norm(a), norm(b)
    ax = np.cross(a, b)
    s = np.linalg.norm(ax)
    if s < 1e-9:
        return np.eye(3)
    return rot_axis(ax, np.arctan2(s, np.dot(a, b)))


def bone_frame(direction, front):
    """Как bone_rot в rig.rs: ось -Y смотрит по direction, -Z — по front."""
    y = -norm(direction)
    f = front - np.dot(front, -y) * -y
    z = -norm(f)
    x = norm(np.cross(y, z))
    z = np.cross(x, y)
    return np.stack([x, y, z], axis=1)


# Какие кости MakeHuman в какую кость игры (с долями).
def merge_map(name):
    side = "r" if name.endswith(".R") else "l"
    base = name.split(".")[0]
    if base in ("root", "pelvis", "spine05"):
        return {"pelvis": 1.0}
    if base == "spine04":
        return {"pelvis": 0.55, "torso": 0.45}
    if base in ("spine03", "spine02", "spine01", "breast") or base.startswith("levator"):
        return {"torso": 1.0}
    if base == "clavicle":
        return {"torso": 0.85, f"upper_arm_{side}": 0.15}
    if base == "shoulder01":
        return {"torso": 0.1, f"upper_arm_{side}": 0.9}
    if base.startswith("upperarm"):
        return {f"upper_arm_{side}": 1.0}
    if base.startswith("lowerarm"):
        return {f"forearm_{side}": 1.0}
    if base in ("wrist",) or base.startswith("metacarpal") or base.startswith("finger"):
        return {f"hand_{side}": 1.0}
    if base.startswith("upperleg"):
        return {f"thigh_{side}": 1.0}
    if base.startswith("lowerleg"):
        return {f"shin_{side}": 1.0}
    if base == "foot" or base.startswith("toe"):
        return {f"foot_{side}": 1.0}
    if base == "neck01":
        return {"torso": 0.5, "head": 0.5}
    return {"head": 1.0}


class HumanRig:
    def __init__(self, base, fist=1.0):
        self.base = base
        nv = len(base.v)
        # Веса MakeHuman: матрица (вершины x кости MH).
        self.mh_names = list(base.mh_weights.keys())
        W = np.zeros((nv, len(self.mh_names)))
        for j, n in enumerate(self.mh_names):
            for i, w in base.mh_weights[n]:
                W[i, j] = w
        s = W.sum(1, keepdims=True)
        W = np.where(s > 0, W / np.maximum(s, 1e-9), 0)
        self.mh_W = W
        if fist > 0:
            self.make_fists(fist)
        self.compute_joints()
        # Веса игры.
        G = np.zeros((nv, len(BONES)))
        for j, n in enumerate(self.mh_names):
            for b, k in merge_map(n).items():
                G[:, B[b]] += W[:, j] * k
        self.W = self.smooth(G)

    def smooth(self, G):
        """Сглаживает веса по соседям: у плеч сильнее (иначе при опущенной руке плечо ломается углом)."""
        edges = []
        for f in self.base.faces:
            for a, b in zip(f, f[1:] + f[:1]):
                edges.append((a, b))
        e = np.array(edges)
        e = np.r_[e, e[:, ::-1]]
        nv = len(G)
        deg = np.bincount(e[:, 0], minlength=nv).astype(float)
        v = self.base.v
        shoulder = np.exp(-((np.abs(v[:, 0]) - 0.19) ** 2 / 0.006 + (v[:, 1] - 1.38) ** 2 / 0.006))
        hand = np.isin(np.argmax(G, axis=1), [B["hand_r"], B["hand_l"]])
        for it in range(30):
            acc = np.zeros_like(G)
            np.add.at(acc, e[:, 0], G[e[:, 1]])
            avg = np.where(deg[:, None] > 0, acc / np.maximum(deg, 1)[:, None], G)
            k = (0.5 * shoulder + (0.35 if it < 4 else 0.0))[:, None]
            k = np.where(hand[:, None], 0.0, k)
            G = G * (1 - k) + avg * k
        return G / np.maximum(G.sum(1, keepdims=True), 1e-9)

    # -----------------------------------------------------------------------------------------
    def mh_bone(self, n):
        return self.base.bone(n)

    def mh_fk(self, local):
        """local: {кость MH: 3x3 поворот вокруг её головы в осях привязки}. Деформирует меш (LBS)."""
        bones = self.base.skel["bones"]
        heads = {n: self.base.joint(bones[n]["head"]) for n in bones}
        glob = {}

        def g(n):
            if n in glob:
                return glob[n]
            p = bones[n].get("parent")
            M = np.eye(4)
            if n in local:
                h = heads[n]
                L = np.eye(4)
                L[:3, :3] = local[n]
                L[:3, 3] = h - local[n] @ h
                M = L
            if p:
                M = g(p) @ M
            glob[n] = M
            return M

        v = self.base.v
        out = np.zeros_like(v)
        vh = np.c_[v, np.ones(len(v))]
        for j, n in enumerate(self.mh_names):
            w = self.mh_W[:, j]
            idx = np.nonzero(w)[0]
            if len(idx) == 0:
                continue
            M = g(n) if n in bones else np.eye(4)
            out[idx] += w[idx, None] * (vh[idx] @ M.T)[:, :3]
        moved = self.mh_W.sum(1) > 0
        self.base.v = np.where(moved[:, None], out, v)

    def hand_axes(self, side):
        """Оси кисти: along — от запястья к пальцам, across — от указательного к мизинцу, palm — нормаль ладони."""
        S = side.upper()
        wrist = self.base.joint(self.base.skel["bones"][f"wrist.{S}"]["head"])
        idx = self.base.joint(self.base.skel["bones"][f"finger2-1.{S}"]["head"])
        mid = self.base.joint(self.base.skel["bones"][f"finger3-1.{S}"]["head"])
        pinky = self.base.joint(self.base.skel["bones"][f"finger5-1.{S}"]["head"])
        along = norm(mid - wrist)
        across = norm(pinky - idx)
        across = norm(across - np.dot(across, along) * along)
        palm = np.cross(along, across)
        if side == "l":
            palm = -palm
        # Ладонь в A-позе смотрит вниз — проверяем знак по этому.
        if palm[1] > 0:
            palm = -palm
        return wrist, along, across, palm

    def make_fists(self, k):
        local = {}
        for side in ("r", "l"):
            S = side.upper()
            wrist, along, across, palm = self.hand_axes(side)
            # Сгибаем пальцы к ладони: ось сгиба — поперёк кисти, знак — чтобы палец шёл к ладони.
            axis = np.cross(along, palm)
            curl = [1.25, 1.55, 0.95]
            spread = {2: -0.04, 3: 0.0, 4: 0.05, 5: 0.12}
            for f in (2, 3, 4, 5):
                for p in (1, 2, 3):
                    ang = curl[p - 1] * k * (1.0 if f != 5 else 1.05)
                    R = rot_axis(axis, ang)
                    if p == 1:
                        R = R @ rot_axis(palm, spread[f] * k)
                    local[f"finger{f}-{p}.{S}"] = R
        self.mh_fk(local)
        # Большой палец ложится поверх средних фаланг указательного и среднего пальцев.
        local = {}
        hb = lambda n: self.base.joint(self.base.skel["bones"][n]["head"])
        tb = lambda n: self.base.joint(self.base.skel["bones"][n]["tail"])
        for side in ("r", "l"):
            S = side.upper()
            wrist, along, across, palm = self.hand_axes(side)
            h1 = hb(f"finger1-1.{S}")
            tip = tb(f"finger1-3.{S}")
            target = 0.6 * tb(f"finger2-2.{S}") + 0.4 * tb(f"finger3-2.{S}") - along * 0.012 * k
            R = rot_arc(tip - h1, h1 + (target - h1) * k + (tip - h1) * (1 - k) - h1)
            local[f"finger1-1.{S}"] = R
        self.mh_fk(local)

    # -----------------------------------------------------------------------------------------
    def compute_joints(self):
        b = self.base
        J = {}
        J["pelvis"] = b.joint(b.skel["bones"]["spine05"]["head"])
        J["torso"] = b.joint(b.skel["bones"]["spine03"]["head"])
        J["head"] = b.joint(b.skel["bones"]["neck01"]["head"])
        for s in ("r", "l"):
            S = s.upper()
            hb = lambda n: b.joint(b.skel["bones"][n]["head"])
            # Центр плечевого сустава глубже, чем голова кости MakeHuman: так опущенная рука не «ломает» дельту.
            J[f"upper_arm_{s}"] = hb(f"upperarm01.{S}") + np.array([-0.012 if s == "r" else 0.012, -0.015, 0.0])
            J[f"forearm_{s}"] = hb(f"lowerarm01.{S}")
            J[f"hand_{s}"] = hb(f"wrist.{S}")
            J[f"thigh_{s}"] = hb(f"upperleg01.{S}")
            J[f"shin_{s}"] = hb(f"lowerleg01.{S}")
            J[f"foot_{s}"] = hb(f"foot.{S}")
        # Симметрия: левую сторону зеркалим с правой, чтобы скелет был ровным.
        for n in BONES:
            if n.endswith("_l"):
                r = J[n[:-1] + "r"]
                J[n] = np.array([-r[0], r[1], r[2]])
        for n in ("pelvis", "torso", "head"):
            J[n][0] = 0.0
        self.J = J
        # Поворот привязки для каждой кости.
        R = {n: np.eye(3) for n in BONES}
        for s in ("r", "l"):
            sh, el, wr = J[f"upper_arm_{s}"], J[f"forearm_{s}"], J[f"hand_{s}"]
            _, along, across, palm = self.hand_axes(s)
            R[f"upper_arm_{s}"] = bone_frame(el - sh, wr - el)
            # Предплечье и кисть: -Y по предплечью, ладонь (у правой) смотрит в -X кости.
            sign = 1.0 if s == "r" else -1.0
            fore = norm(wr - el)
            x = -palm * sign
            x = norm(x - np.dot(x, fore) * fore)
            y = -fore
            z = np.cross(x, y)
            R[f"forearm_{s}"] = np.stack([x, y, z], axis=1)
            R[f"hand_{s}"] = R[f"forearm_{s}"]
            hp, kn, an = J[f"thigh_{s}"], J[f"shin_{s}"], J[f"foot_{s}"]
            R[f"thigh_{s}"] = bone_frame(kn - hp, np.array([0, 0, -1.0]))
            R[f"shin_{s}"] = bone_frame(an - kn, np.array([0, 0, -1.0]))
        self.R = R
        self.lengths = {
            "upper_arm": float(np.linalg.norm(J["forearm_r"] - J["upper_arm_r"])),
            "forearm": float(np.linalg.norm(J["hand_r"] - J["forearm_r"])),
            "thigh": float(np.linalg.norm(J["shin_r"] - J["thigh_r"])),
            "shin": float(np.linalg.norm(J["foot_r"] - J["shin_r"])),
        }

    def rest_offsets(self):
        """Смещения суставов от родителя в покое игры (конечности висят прямо вниз)."""
        J, L = self.J, self.lengths
        off = {}
        off["pelvis"] = J["pelvis"]
        off["torso"] = J["torso"] - J["pelvis"]
        off["head"] = J["head"] - J["torso"]
        for s in ("r", "l"):
            off[f"upper_arm_{s}"] = J[f"upper_arm_{s}"] - J["torso"]
            off[f"forearm_{s}"] = np.array([0, -L["upper_arm"], 0])
            off[f"hand_{s}"] = np.array([0, -L["forearm"], 0])
            off[f"thigh_{s}"] = J[f"thigh_{s}"] - J["pelvis"]
            off[f"shin_{s}"] = np.array([0, -L["thigh"], 0])
            off[f"foot_{s}"] = np.array([0, -L["shin"], 0])
        return off

    def inverse_binds(self):
        """Обратные матрицы привязки (4x4, столбцами как в glTF)."""
        out = []
        for n in BONES:
            M = np.eye(4)
            M[:3, :3] = self.R[n]
            M[:3, 3] = self.J[n]
            out.append(np.linalg.inv(M))
        return out

    def rest_world(self):
        """Мировые матрицы костей в покое игры."""
        off = self.rest_offsets()
        mats = []
        for i, n in enumerate(BONES):
            T = np.eye(4)
            T[:3, 3] = off[n]
            mats.append(T if PARENT[i] is None else mats[PARENT[i]] @ T)
        return mats

    def skin(self, v, W, mats):
        """LBS: v (N,3), W (N,15), mats — мировые матрицы костей."""
        ib = self.inverse_binds()
        out = np.zeros_like(v)
        vh = np.c_[v, np.ones(len(v))]
        for j in range(len(BONES)):
            idx = np.nonzero(W[:, j])[0]
            if len(idx):
                M = mats[j] @ ib[j]
                out[idx] += W[idx, j, None] * (vh[idx] @ M.T)[:, :3]
        return out

    def grip_point(self, side):
        """Центр кулака (где рукоять) и ось отверстия кулака — в системе кисти игры."""
        b = self.base
        S = side.upper()
        hb = lambda n: b.joint(b.skel["bones"][n]["head"])
        # Середина между основаниями средних фаланг и ладонью: там проходит рукоять.
        knuck = np.mean([hb(f"finger{f}-1.{S}") for f in (2, 3, 4, 5)], axis=0)
        tips = np.mean([b.joint(b.skel["bones"][f"finger{f}-2.{S}"]["tail"]) for f in (2, 3, 4, 5)], axis=0)
        c = 0.5 * (knuck + tips)
        axis = norm(hb(f"finger5-1.{S}") - hb(f"finger2-1.{S}"))
        Minv = np.linalg.inv(np.r_[np.c_[self.R[f"hand_{side}"], self.J[f"hand_{side}"]], [[0, 0, 0, 1]]])
        lc = (Minv @ np.r_[c, 1])[:3]
        la = self.R[f"hand_{side}"].T @ axis
        return lc, la
