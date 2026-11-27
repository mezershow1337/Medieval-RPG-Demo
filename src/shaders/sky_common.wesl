// Общее небо: его рисует купол неба (sky.wesl), в нём отражается море (water.wesl),
// и той же формулой на процессоре строится карта окружения для отражений и рассеянного
// света (graphics::sky_env). Цвета — в единицах экрана (как у купола, без экспозиции).
//
// Золотой час: глубокая синева в зените, золото и персик у горизонта со стороны солнца,
// кучевые облака с золотыми краями, высокие перистые облака.
// Закат и рассвет (солнце у самого горизонта и чуть под ним): облака подсвечены снизу —
// у солнца оранжевые и золотые, выше малиновые и ярко-розовые; над ними, в тени Земли, —
// сиренево-серые; у горизонта под солнцем — узкая оранжево-жёлтая полоса зари. Розовее
// всего пелена высококучевых облаков (выше кучевых — солнце светит на неё дольше).
// День: голубое небо с белыми облаками. Ночь: тёмная синева, луна и звёзды, облака в
// лунном свете. Между ними — плавно, по часам мира (graphics::mood_at).

struct SkyParams {
    // xyz — направление на солнце, w — «день» 0..1 (1 днём, 0 на закате и в сумерках).
    sun: vec4<f32>,
    // Цвет горизонта со стороны от солнца (он же цвет дымки), w — облачность 0..1.
    horizon: vec4<f32>,
    // Добавка к горизонту в сторону солнца (как у тумана Bevy), w — показатель резкости.
    horizon_sun: vec4<f32>,
    // Цвет зенита, w — время сдвига облаков (для отражений можно 0).
    zenith: vec4<f32>,
    // xyz — направление на луну, w — освещённая часть диска 0..1.
    moon: vec4<f32>,
    // x — ночь 0..1, y — ненастье (серая пелена туч) 0..1, zw — ветер, м/с (снос облаков).
    extra: vec4<f32>,
    // Сумерки: rgb — розово-коралловое небо над зарёй со стороны солнца, w — пелена
    // высококучевых облаков 0..1.
    glow: vec4<f32>,
    // Сумерки: rgb — сиреневое небо выше и в стороне от солнца (им же освещены тени облаков),
    // w — сила подсветки облаков снизу низким солнцем.
    tint: vec4<f32>,
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        v += a * vnoise(q);
        q = q * 2.02 + vec2<f32>(5.3, 1.7);
        a *= 0.5;
    }
    return v;
}

fn fbm3(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.57 + vnoise(p * 2.03 + 3.1) * 0.29 + vnoise(p * 4.1 + 7.7) * 0.14;
}

// Ячеистый шум (Ворли): расстояние до ближайшей точки, 0..~1. Его «обратная» сторона даёт
// круглые клубы — как у кучевых облаков (приём из облаков Horizon Zero Dawn: Перлин + Ворли).
fn worley(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var d = 1.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let g = vec2<f32>(f32(x), f32(y));
            let o = vec2<f32>(hash21(i + g), hash21(i + g + 17.3));
            let r = g + o - f;
            d = min(d, dot(r, r));
        }
    }
    return sqrt(d);
}

// Плотность кучевого облака в точке плоскости: крупная форма, выеденная клубами.
fn cloud_density(q: vec2<f32>) -> f32 {
    let base = fbm(q * 1.3);
    let puffs = 1.0 - worley(q * 3.4);
    let fine = 1.0 - worley(q * 8.3 + 4.0);
    return base * 0.7 + puffs * 0.26 + fine * 0.09 - 0.115;
}

// Цвет горизонта (и дымки) в направлении `dir`: та же формула, что у тумана Bevy,
// поэтому дальние стены и море уходят в дымку ровно того цвета, что и небо за ними.
fn horizon_color(dir: vec3<f32>, s: SkyParams) -> vec3<f32> {
    let sd = max(dot(normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(1e-5, 0.0, 0.0)), normalize(s.sun.xyz)), 0.0);
    return s.horizon.rgb + s.horizon_sun.rgb * pow(sd, s.horizon_sun.w);
}

// Свет низкого солнца на нижней кромке облаков (rgb) и доходит ли он туда (w, 0..1).
// Чем ниже солнце над этим местом неба, тем длиннее путь его света сквозь воздух и тем он
// краснее: у солнца облака золотые и оранжевые, выше — малиновые и розовые. Облака по ту
// сторону от солнца раньше уходят в тень Земли и становятся сиреневыми; пока солнце садится,
// граница тени ползёт к нему. `reach` — насколько высоко слой: чем выше, тем дольше он
// освещён (высококучевые розовеют и после того, как кучевые погасли).
fn twilight_light(dir: vec3<f32>, sun: vec3<f32>, e: f32, reach: f32) -> vec4<f32> {
    let cos_az = dot(normalize(dir.xz + vec2<f32>(1e-5, 0.0)), normalize(sun.xz + vec2<f32>(1e-4, 0.0)));
    // Высота солнца над местом под облаком (рад): к солнцу выше, от солнца ниже, чем у нас.
    let local = asin(clamp(sun.y, -1.0, 1.0)) + clamp(reach / max(e, 0.02) * cos_az, -0.1, 0.1);
    let lit = smoothstep(-0.022, -0.008, local);
    let k = clamp((local + 0.012) / 0.312, 0.0, 1.0);
    var c = mix(vec3<f32>(1.0, 0.04, 0.09), vec3<f32>(1.0, 0.25, 0.07), smoothstep(0.06, 0.2, k));
    c = mix(c, vec3<f32>(1.0, 0.6, 0.25), smoothstep(0.18, 0.42, k));
    c = mix(c, vec3<f32>(1.0, 0.9, 0.75), smoothstep(0.4, 1.0, k));
    return vec4<f32>(c * (mix(0.8, 1.0, smoothstep(0.0, 0.4, k)) * lit), lit);
}

// Пелена высококучевых облаков вытянута грядами с севера на юг: на закате и рассвете гряды
// лежат поперёк взгляда на солнце, как слоистые облака на фото заката.
const DECK_ROT: vec2<f32> = vec2<f32>(0.17, -0.985);

// Облака: x — плотность покрытия 0..1, yzw — цвет.
fn cloud_layer(dir: vec3<f32>, s: SkyParams, t: f32, hor: vec3<f32>) -> vec4<f32> {
    let e = dir.y;
    if e <= 0.0 {
        return vec4<f32>(0.0);
    }
    let sun = normalize(s.sun.xyz);
    let d = clamp(s.sun.w, 0.0, 1.0);
    let n = clamp(s.extra.x, 0.0, 1.0);
    let oc = clamp(s.extra.y, 0.0, 1.0);
    let cover_k = s.horizon.w;
    let sd = max(dot(dir, sun), 0.0);
    let s2 = normalize(sun.xz + vec2<f32>(1e-4, 0.0));
    let toward = clamp(dot(normalize(dir.xz + vec2<f32>(1e-5, 0.0)), s2) * 0.5 + 0.5, 0.0, 1.0);
    // Плоскость облаков: вдали сжимается к горизонту; облака сносит ветром.
    let drift = vec2<f32>(0.0035, 0.0012) + s.extra.zw * 0.0009;
    let p = dir.xz / (e + 0.07) * 0.75 + drift * t;
    // Искривление пространства — рваные, «живые» края.
    let w = vec2<f32>(fbm3(p * 0.6 + 3.1), fbm3(p * 0.6 + 7.7));
    let q = p + (w - 0.5) * 0.55;
    let dn = cloud_density(q);
    let thr = mix(0.66, 0.44, min(cover_k, 1.0)) - 0.22 * smoothstep(0.7, 1.0, cover_k) - 0.3 * oc;
    // Толща облака: четыре слоя по высоте (луч проходит сквозь пласт — у слоёв параллакс),
    // днище плоское, макушка круглая; свет гаснет в толще по закону Бера — освещённые бока и
    // серые днища получаются сами, как у настоящих кучевых.
    var trans = 1.0;
    var lit_acc = 0.0;
    for (var i = 0; i < 4; i++) {
        let h = (f32(i) + 0.5) / 4.0;
        let qh = q * (1.0 + h * 0.07);
        let thr_h = thr + h * h * 0.17;
        let dh = cloud_density(qh);
        let a = smoothstep(thr_h, thr_h + 0.05, dh) * 0.7;
        // Сколько облака между этой точкой и солнцем (выше и в его сторону).
        let ds = cloud_density(qh + s2 * 0.11) - (thr + (h + 0.3) * (h + 0.3) * 0.17);
        let shade = exp(-max(ds, 0.0) * 9.0) * mix(0.55, 1.0, h);
        lit_acc += trans * a * shade;
        trans *= 1.0 - a;
    }
    let body = 1.0 - trans;
    // В ненастье тучи сливаются в сплошную пелену без просветов.
    var cover = max(body, oc * 0.93) * smoothstep(0.0, 0.07, e);
    var lit = clamp(lit_acc / max(body, 1e-3), 0.0, 1.0);
    var thin = 1.0 - smoothstep(0.25, 0.85, body);
    // На закате и ночью облака прежние (мягкие, с золотой кромкой) — закаты не трогаем;
    // объёмные кучевые проявляются с высотой солнца.
    if d < 0.999 {
        let q0 = p + (w - 0.5) * 1.1;
        let dn0 = fbm(q0 * 1.3);
        let cover0 = max(smoothstep(thr, thr + 0.16, dn0), oc * 0.93) * smoothstep(0.0, 0.07, e);
        let lit0 = clamp((dn0 - fbm((q0 + s2 * 0.16) * 1.3)) * 5.0 + 0.45, 0.0, 1.0);
        let thin0 = 1.0 - smoothstep(thr + 0.05, thr + 0.3, dn0);
        cover = mix(cover0, cover, d);
        lit = mix(lit0, lit, d);
        thin = mix(thin0, thin, d);
    }
    // Солнце над горизонтом (для золотой кромки у солнца).
    let up = smoothstep(-0.03, 0.04, sun.y);
    // Тени облаков в сумерки освещены сиреневым небом.
    let tw_amb = s.tint.rgb * 0.75;
    let moon = normalize(s.moon.xyz);
    let md = max(dot(dir, moon), 0.0);
    let moon_up = smoothstep(-0.02, 0.1, moon.y) * s.moon.w;
    let fade = (1.0 - smoothstep(0.0, 0.22, e)) * 0.55;

    // ---- Кучевые ----
    // День: белые кучевые с серыми днищами.
    var day_c = mix(vec3<f32>(0.5, 0.55, 0.68), vec3<f32>(1.38, 1.36, 1.32), clamp(lit * 0.85 + thin * 0.25, 0.0, 1.0));
    day_c += vec3<f32>(0.8, 0.7, 0.5) * pow(sd, 10.0) * thin;
    // Закат: снизу облака подсвечены низким солнцем, сердцевины темнее, у солнца — золотая кромка.
    let low_sun = twilight_light(dir, sun, e, 0.001);
    var tw_c = tw_amb * (mix(0.9, 0.6, lit) * (1.0 - 0.8 * low_sun.w)) + low_sun.rgb * s.tint.w * mix(0.45, 1.0, lit * (0.35 + 0.65 * thin));
    tw_c += vec3<f32>(4.0, 2.3, 1.0) * pow(sd, 9.0) * thin * 1.6 * up;
    var c = mix(tw_c, day_c, d);
    // Ненастье: тяжёлые серые тучи.
    let grey = mix(vec3<f32>(0.34, 0.35, 0.38), vec3<f32>(0.62, 0.64, 0.68), lit * 0.6 + thin * 0.4) * mix(0.55, 1.0, d);
    c = mix(c, grey, oc * 0.8);
    // Ночь: тёмные облака, края серебрятся в лунном свете.
    var night_c = mix(vec3<f32>(0.010, 0.012, 0.020), vec3<f32>(0.045, 0.052, 0.075), lit) * (0.6 + 0.8 * moon_up);
    night_c += vec3<f32>(0.35, 0.38, 0.45) * pow(md, 12.0) * thin * moon_up;
    c = mix(c, night_c, n);
    // Дальние облака тонут в дымке горизонта.
    c = mix(c, hor, fade);

    // ---- Пелена высококучевых: выше кучевых, гряды и мелкие барашки ----
    var deck = 0.0;
    var cd = vec3<f32>(0.0);
    if s.glow.w > 0.01 {
        let pd = dir.xz / (e + 0.035) * 0.9 + drift * (t * 0.6);
        let r = vec2<f32>(pd.x * DECK_ROT.x + pd.y * DECK_ROT.y, -pd.x * DECK_ROT.y + pd.y * DECK_ROT.x) * vec2<f32>(0.7, 1.35);
        let dd = fbm(r * 1.6 + w * 1.17 + vec2<f32>(11.0, 3.0)) * 0.7 + fbm3(r * 6.0 + w * 1.8) * 0.3;
        let dthr = mix(0.62, 0.26, s.glow.w);
        // Днём пелена тоньше на вид: на ярком небе её почти не видно, на закатном она горит.
        deck = smoothstep(dthr, dthr + 0.14, dd) * smoothstep(0.0, 0.05, e) * min(s.glow.w * 3.0, 1.0) * mix(1.0, 0.55, d) * (1.0 - oc * 0.5);
        let thick = smoothstep(dthr + 0.1, dthr + 0.3, dd);
        let hi_sun = twilight_light(dir, sun, e, 0.0015);
        // Гуще — темнее и сиреневее: сердцевины гряд почти не пропускают свет снизу. Крупными
        // пятнами гряды в тени соседних облаков — серо-сиреневые, между ними горят розовым.
        let sunny = smoothstep(0.36, 0.62, w.y);
        // Яркость подсветки мягко упирается в потолок: самые светлые гряды остаются насыщенно-розовыми,
        // а не выгорают в бело-розовый.
        let glow_k = hi_sun.w * s.tint.w * 1.25 * mix(1.1, 0.25, thick) * mix(0.4, 1.3, sunny);
        let tw_d = tw_amb * (mix(1.0, 0.75, thick) * (1.0 - 0.8 * hi_sun.w * (1.0 - 0.6 * thick) * sunny)) + hi_sun.rgb * (2.0 * (1.0 - exp(-glow_k / 2.0)) / max(hi_sun.w, 1e-3));
        let day_d = mix(vec3<f32>(1.15, 1.15, 1.18), vec3<f32>(0.75, 0.78, 0.86), thick) + vec3<f32>(0.5, 0.45, 0.35) * pow(sd, 10.0) * (1.0 - thick);
        cd = mix(tw_d, day_d, d);
        cd = mix(cd, grey, oc * 0.8);
        let night_d = mix(vec3<f32>(0.045, 0.052, 0.075), vec3<f32>(0.010, 0.012, 0.020), thick) * (0.6 + 0.8 * moon_up) + vec3<f32>(0.3, 0.33, 0.4) * pow(md, 12.0) * (1.0 - thick) * moon_up;
        cd = mix(cd, night_d, n);
        cd = mix(cd, hor, fade * 0.5);
    }

    // Перистые облака — тонкие штрихи высоко в небе (в ненастье их не видно за тучами).
    let pc = dir.xz / (e + 0.3) * 0.55 + vec2<f32>(t * 0.002, 0.0) + s.extra.zw * (t * 0.0004);
    let rp = vec2<f32>(pc.x * 0.8 + pc.y * 0.6, -pc.x * 0.6 + pc.y * 0.8);
    let streak = fbm(vec2<f32>(rp.x * 0.45, rp.y * 5.0) + w * 0.6);
    let cirrus = smoothstep(0.56, 0.78, streak) * smoothstep(0.04, 0.25, e) * 0.35 * (1.0 - cover) * (1.0 - deck) * (1.0 - oc);
    var cc = mix(mix(vec3<f32>(1.0, 0.55, 0.6), vec3<f32>(2.3, 1.2, 0.6), pow(toward, 1.5)), vec3<f32>(1.1, 1.12, 1.15), d);
    cc = mix(cc, vec3<f32>(0.04, 0.045, 0.06) * (0.6 + moon_up), n);
    // Кучевые впереди пелены, перистые — в просветах.
    let back = deck * (1.0 - cover);
    let total = cover + back + cirrus;
    let col = (c * cover + cd * back + cc * cirrus) / max(total, 1e-4);
    return vec4<f32>(min(total, 1.0), col);
}

// Небо в направлении `dir`. `disc` — рисовать ли диски солнца и луны (в отражениях их
// заменяет блик).
fn sky_radiance(dir_in: vec3<f32>, s: SkyParams, t: f32, disc: bool) -> vec3<f32> {
    var cover = 0.0;
    return sky_radiance_cover(dir_in, s, t, disc, &cover);
}

// То же, и сколько неба закрыто облаками (купол прячет за ними звёзды — облака не считаем дважды).
fn sky_radiance_cover(dir_in: vec3<f32>, s: SkyParams, t: f32, disc: bool, cover: ptr<function, f32>) -> vec3<f32> {
    let dir = normalize(dir_in);
    let sun = normalize(s.sun.xyz);
    let d = clamp(s.sun.w, 0.0, 1.0);
    let n = clamp(s.extra.x, 0.0, 1.0);
    let oc = clamp(s.extra.y, 0.0, 1.0);
    let e = dir.y;
    let h = clamp(e, 0.0, 1.0);
    let sd = max(dot(dir, sun), 0.0);
    let hor = horizon_color(dir, s);

    // Градиент: горизонт → середина → зенит. В сумерки середина неба со стороны солнца —
    // коралловая (над зарёй), выше и в стороне — сиреневая. Ночью — от дымки к густой синеве.
    let s2 = normalize(sun.xz + vec2<f32>(1e-4, 0.0));
    let toward = clamp(dot(normalize(dir.xz + vec2<f32>(1e-5, 0.0)), s2) * 0.5 + 0.5, 0.0, 1.0);
    let tw_mid = mix(s.tint.rgb, s.glow.rgb, pow(toward, 2.5) * (1.0 - smoothstep(0.03, 0.2, h)));
    var mid = mix(tw_mid, vec3<f32>(0.26, 0.46, 0.86), d);
    mid = mix(mid, mix(hor, s.zenith.rgb, 0.5), n);
    // Ненастье: небо ровное и серое.
    mid = mix(mid, hor * 1.04, oc * 0.7);
    var col = mix(hor, mid, smoothstep(0.0, mix(0.10, 0.16, d), h));
    col = mix(col, mix(s.zenith.rgb, hor * 1.02, oc * 0.6), smoothstep(0.12, mix(0.5, 0.85, d), h));
    // Заря: узкая оранжево-жёлтая полоса у самого горизонта под садящимся (встающим) солнцем.
    let el = sun.y;
    let veil = (1.0 - n) * (1.0 - 0.8 * oc);
    let zarya = smoothstep(-0.1, -0.035, el) * (1.0 - smoothstep(0.03, 0.12, el)) * veil;
    let zarya_c = mix(vec3<f32>(1.4, 0.25, 0.04), vec3<f32>(2.2, 0.75, 0.08), smoothstep(-0.07, -0.02, el));
    col += zarya_c * (pow(toward, 6.0) * (1.0 - smoothstep(0.0, 0.012, h)) * zarya);

    // Ореол вокруг солнца (рассеяние Ми): у горизонта сильнее и краснее; за тучами — тусклый.
    let low = smoothstep(-0.05, 0.05, el);
    let halo_gold = (mix(vec3<f32>(1.6, 0.45, 0.12), vec3<f32>(2.2, 1.15, 0.45), smoothstep(0.0, 0.1, el)) * pow(sd, 7.0) * 0.55 + vec3<f32>(3.5, 2.0, 0.9) * pow(sd, 90.0) * 0.9) * low;
    let halo_day = vec3<f32>(1.0, 0.9, 0.7) * pow(sd, 8.0) * 0.5;
    col += mix(halo_gold, halo_day, d) * veil;
    // Луна: ореол в дымке и диск.
    let moon = normalize(s.moon.xyz);
    let md = max(dot(dir, moon), 0.0);
    let moon_up = smoothstep(-0.03, 0.05, moon.y) * s.moon.w * n;
    col += vec3<f32>(0.10, 0.12, 0.16) * pow(md, 60.0) * moon_up * (1.0 - 0.6 * oc);
    if disc {
        let sun_disc = smoothstep(0.99985, 0.99993, sd) * (1.0 - oc * 0.9) * smoothstep(-0.03, 0.0, sun.y);
        col += mix(vec3<f32>(40.0, 26.0, 13.0), vec3<f32>(30.0, 27.0, 22.0), d) * sun_disc;
        let moon_disc = smoothstep(0.99980, 0.99990, md) * (1.0 - oc * 0.95);
        col += vec3<f32>(2.4, 2.5, 2.7) * moon_disc * moon_up;
    }

    // Облака.
    let cl = cloud_layer(dir, s, s.zenith.w, hor);
    col = mix(col, cl.yzw, cl.x);
    *cover = cl.x;

    // Горы Таврики на юго-востоке (+X, +Z): голубые в дымке, на закате — с розовым отсветом,
    // ночью — чёрный силуэт на чуть светлом небе.
    // Гребни не выше 0,085 рад — выше горы не считаем.
    if e < 0.09 {
        let az = atan2(dir.z, dir.x);
        let land = smoothstep(-0.2, 0.5, dot(normalize(vec2<f32>(dir.x, dir.z) + 1e-5), normalize(vec2<f32>(0.45, 1.0))));
        let ridge = (0.012 + 0.05 * fbm(vec2<f32>(az * 3.0, 1.0)) + 0.02 * fbm(vec2<f32>(az * 12.0, 5.0))) * land;
        let near_ridge = (0.004 + 0.022 * fbm(vec2<f32>(az * 6.0 + 9.0, 2.0))) * land;
        var far_c = mix(mix(hor * 0.8, vec3<f32>(0.62, 0.44, 0.50), 0.5), vec3<f32>(0.52, 0.60, 0.72), d);
        far_c = mix(far_c, hor * 0.92, oc * 0.5);
        far_c = mix(far_c, hor * 0.55, n);
        col = mix(col, far_c, (1.0 - smoothstep(ridge - 0.002, ridge + 0.002, e)) * 0.9);
        col = mix(col, far_c * 0.72, (1.0 - smoothstep(near_ridge - 0.002, near_ridge + 0.002, e)) * 0.9);
    }
    if e < 0.0 {
        // Ниже горизонта — дымка (туман Bevy того же цвета закрывает здесь землю и море).
        col = hor;
    }
    return col;
}
