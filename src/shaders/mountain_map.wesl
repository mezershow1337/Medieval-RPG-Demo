// Панорама дальних гор (см. mountain_map.rs): по горизонтали — азимут в сторону суши, по
// вертикали — высота над горизонтом. Для каждой точки — луч по карте высот гор
// (`mountain_shade` в sky_common): склон, лес, скала, снег, свет солнца и дымка. Рисуется
// столбцами, понемногу; купол неба и море её только читают.

import medieval_rpg::shaders::sky_common::{SkyParams, MountainView, mountain_shade, mountain_map_dir};

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: SkyParams;
// xy — размер панорамы в точках.
@group(constants::MATERIAL_BIND_GROUP) @binding(1)
var<uniform> view: vec4<f32>;
// Откуда смотрим (см. MountainView): горы считаются от игрока.
@group(constants::MATERIAL_BIND_GROUP) @binding(2)
var<uniform> mview: MountainView;

@fragment
fn fragment(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    // Четыре луча на точку: кромка гребня на фоне неба выходит гладкой, а не лесенкой.
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let off = vec2<f32>(f32(i % 2) - 0.5, f32(i / 2) - 0.5) * 0.5 + vec2<f32>(0.125, -0.125) * (f32(i) - 1.5);
        let m = mountain_shade(mountain_map_dir((frag.xy + off) / view.xy), sky, mview);
        acc += vec4<f32>(m.rgb * m.a, m.a);
    }
    acc *= 0.25;
    // Свет пишем с множителем 0,5 (снег на солнце ярче единицы), уже умноженный на покрытие.
    return vec4<f32>(clamp(acc.rgb * 0.5, vec3<f32>(0.0), vec3<f32>(1.0)), acc.a);
}
