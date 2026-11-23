// Общее небо: его рисует купол неба (sky.wesl), в нём отражается море (water.wesl),
// и той же формулой на процессоре строится карта окружения для отражений и рассеянного
// света (graphics::sky_env). Цвета — в единицах экрана (как у купола, без экспозиции).
//
// Закат («золотой час»): глубокая синева в зените, золото и персик у горизонта со стороны
// солнца, розоватый «пояс Венеры» и сизая дымка с противоположной стороны, кучевые облака,
// подсвеченные снизу (тёмные сердцевины, золотые края), высокие перистые облака.
// День: голубое небо с белыми облаками.

struct SkyParams {
    // xyz — направление на солнце, w — 1 днём, 0 на закате.
    sun: vec4<f32>,
    // Цвет горизонта со стороны от солнца (он же цвет дымки), w — облачность 0..1.
    horizon: vec4<f32>,
    // Добавка к горизонту в сторону солнца (как у тумана Bevy), w — показатель резкости.
    horizon_sun: vec4<f32>,
    // Цвет зенита, w — время сдвига облаков (для отражений можно 0).
    zenith: vec4<f32>,
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

// Цвет горизонта (и дымки) в направлении `dir`: та же формула, что у тумана Bevy,
// поэтому дальние стены и море уходят в дымку ровно того цвета, что и небо за ними.
fn horizon_color(dir: vec3<f32>, s: SkyParams) -> vec3<f32> {
    let sd = max(dot(normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(1e-5, 0.0, 0.0)), normalize(s.sun.xyz)), 0.0);
    return s.horizon.rgb + s.horizon_sun.rgb * pow(sd, s.horizon_sun.w);
}

// Облака: x — плотность покрытия 0..1, yzw — цвет.
fn cloud_layer(dir: vec3<f32>, sun: vec3<f32>, t: f32, cover_k: f32, day: bool, hor: vec3<f32>) -> vec4<f32> {
    let e = dir.y;
    if e <= 0.0 {
        return vec4<f32>(0.0);
    }
    let sd = max(dot(dir, sun), 0.0);
    let s2 = normalize(sun.xz + vec2<f32>(1e-4, 0.0));
    let toward = clamp(dot(normalize(dir.xz + vec2<f32>(1e-5, 0.0)), s2) * 0.5 + 0.5, 0.0, 1.0);
    // Плоскость облаков: вдали сжимается к горизонту.
    let p = dir.xz / (e + 0.07) * 0.75 + vec2<f32>(t * 0.0035, t * 0.0012);
    // Искривление пространства — рваные, «живые» края.
    let w = vec2<f32>(fbm3(p * 0.6 + 3.1), fbm3(p * 0.6 + 7.7));
    let q = p + (w - 0.5) * 1.1;
    let d = fbm(q * 1.3);
    let thr = mix(0.66, 0.44, cover_k);
    let cover = smoothstep(thr, thr + 0.16, d) * smoothstep(0.0, 0.07, e);
    // Самозатенение: насколько облако гуще в сторону солнца.
    let d_sun = fbm((q + s2 * 0.16) * 1.3);
    let lit = clamp((d - d_sun) * 5.0 + 0.45, 0.0, 1.0);
    let thin = 1.0 - smoothstep(thr + 0.05, thr + 0.3, d);
    var c: vec3<f32>;
    if day {
        c = mix(vec3<f32>(0.62, 0.66, 0.76), vec3<f32>(1.32, 1.3, 1.26), lit * 0.7 + thin * 0.3);
        c += vec3<f32>(0.8, 0.7, 0.5) * pow(sd, 10.0) * thin;
    } else {
        // Снизу облака подсвечены закатом: у солнца — золото, над головой — персик и розовый,
        // напротив — сизо-лиловые; сердцевины тёмные.
        let gold = mix(vec3<f32>(1.25, 0.62, 0.55), vec3<f32>(2.9, 1.45, 0.55), pow(toward, 2.0));
        let shadow = mix(vec3<f32>(0.26, 0.27, 0.40), vec3<f32>(0.42, 0.26, 0.26), toward);
        c = mix(shadow, gold, lit * (0.35 + 0.65 * thin));
        // Серебристая (золотая) кромка у облаков рядом с солнцем.
        c += vec3<f32>(4.0, 2.3, 1.0) * pow(sd, 9.0) * thin * 1.6;
    }
    // Дальние облака тонут в дымке горизонта.
    c = mix(c, hor, (1.0 - smoothstep(0.0, 0.22, e)) * 0.55);
    // Перистые облака — тонкие штрихи высоко в небе.
    let pc = dir.xz / (e + 0.3) * 0.55 + vec2<f32>(t * 0.002, 0.0);
    let rp = vec2<f32>(pc.x * 0.8 + pc.y * 0.6, -pc.x * 0.6 + pc.y * 0.8);
    let streak = fbm(vec2<f32>(rp.x * 0.45, rp.y * 5.0) + w * 0.6);
    let cirrus = smoothstep(0.56, 0.78, streak) * smoothstep(0.04, 0.25, e) * 0.55 * (1.0 - cover);
    var cc: vec3<f32>;
    if day {
        cc = vec3<f32>(1.1, 1.12, 1.15);
    } else {
        cc = mix(vec3<f32>(1.0, 0.55, 0.6), vec3<f32>(2.3, 1.2, 0.6), pow(toward, 1.5));
    }
    let total = cover + cirrus;
    let col = (c * cover + cc * cirrus) / max(total, 1e-4);
    return vec4<f32>(min(total, 1.0), col);
}

// Небо в направлении `dir`. `disc` — рисовать ли диск солнца (в отражениях его заменяет блик).
fn sky_radiance(dir_in: vec3<f32>, s: SkyParams, t: f32, disc: bool) -> vec3<f32> {
    let dir = normalize(dir_in);
    let sun = normalize(s.sun.xyz);
    let day = s.sun.w > 0.5;
    let e = dir.y;
    let h = clamp(e, 0.0, 1.0);
    let sd = max(dot(dir, sun), 0.0);
    let hor = horizon_color(dir, s);

    // Градиент: горизонт → середина → зенит. На закате середина неба со стороны солнца —
    // персиковая, с противоположной — лиловато-голубая.
    let s2 = normalize(sun.xz + vec2<f32>(1e-4, 0.0));
    let toward = clamp(dot(normalize(dir.xz + vec2<f32>(1e-5, 0.0)), s2) * 0.5 + 0.5, 0.0, 1.0);
    var mid: vec3<f32>;
    if day {
        mid = vec3<f32>(0.26, 0.46, 0.86);
    } else {
        mid = mix(vec3<f32>(0.34, 0.40, 0.66), vec3<f32>(1.0, 0.66, 0.46), pow(toward, 3.0));
    }
    var col = mix(hor, mid, smoothstep(0.0, select(0.22, 0.16, day), h));
    col = mix(col, s.zenith.rgb, smoothstep(0.12, 0.85, h));

    // Ореол вокруг солнца (рассеяние Ми): у горизонта сильнее.
    if day {
        col += vec3<f32>(1.0, 0.9, 0.7) * pow(sd, 8.0) * 0.5;
    } else {
        col += vec3<f32>(2.2, 1.15, 0.45) * pow(sd, 7.0) * 0.55;
        col += vec3<f32>(3.5, 2.0, 0.9) * pow(sd, 90.0) * 0.9;
    }
    if disc {
        col += select(vec3<f32>(40.0, 26.0, 13.0), vec3<f32>(30.0, 27.0, 22.0), day) * smoothstep(0.99985, 0.99993, sd);
    }

    // Облака.
    let cl = cloud_layer(dir, sun, s.zenith.w, s.horizon.w, day, hor);
    col = mix(col, cl.yzw, cl.x);

    // Горы Таврики на юго-востоке (+X, +Z): голубые в дымке, на закате — с розовым отсветом.
    let az = atan2(dir.z, dir.x);
    let land = smoothstep(-0.2, 0.5, dot(normalize(vec2<f32>(dir.x, dir.z) + 1e-5), normalize(vec2<f32>(0.45, 1.0))));
    let ridge = (0.012 + 0.05 * fbm(vec2<f32>(az * 3.0, 1.0)) + 0.02 * fbm(vec2<f32>(az * 12.0, 5.0))) * land;
    let near_ridge = (0.004 + 0.022 * fbm(vec2<f32>(az * 6.0 + 9.0, 2.0))) * land;
    var far_c: vec3<f32>;
    if day {
        far_c = vec3<f32>(0.52, 0.60, 0.72);
    } else {
        far_c = mix(hor * 0.8, vec3<f32>(0.62, 0.44, 0.50), 0.5);
    }
    col = mix(col, far_c, (1.0 - smoothstep(ridge - 0.002, ridge + 0.002, e)) * 0.9);
    col = mix(col, far_c * 0.72, (1.0 - smoothstep(near_ridge - 0.002, near_ridge + 0.002, e)) * 0.9);
    if e < 0.0 {
        // Ниже горизонта — дымка (туман Bevy того же цвета закрывает здесь землю и море).
        col = hor;
    }
    return col;
}
