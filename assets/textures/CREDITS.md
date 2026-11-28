# Происхождение текстур

`surface/` собирает `tools/textures/build_surface_textures.py` из фотоматериалов
[ambientCG](https://ambientcg.com) и фотосканов [Poly Haven](https://polyhaven.com). Все они
выпущены под **CC0 1.0** (общественное достояние, https://docs.ambientcg.com/license/,
https://polyhaven.com/license): можно использовать, менять и распространять без ограничений.
Слой `roof` — своя работа: черепица VI века (плоские тегулы с бортиками и полукруглые калиптеры
над стыками, как в Херсонесе того времени) смоделирована и запечена в Blender скриптом
`tools/textures/blender_byz_roof.py`. Поверхность глины для неё собрана
`tools/textures/quilt_clay.py` из кусочков фотоскана Poly Haven `clay_roof_tiles_02` (без его
рисунка черепицы), поры — карта высот `clay_plaster`, пятна грязи — `worn_mossy_plasterwall`
(все CC0). Слой `thatch` — тоже своя работа целиком: пучки стеблей соломы и камыша смоделированы
кодом и запечены в Blender скриптом `tools/textures/blender_thatch.py`, без фото.
Ассеты сторонних игр не используются.

| Слой | Материал | Что это |
|---|---|---|
| ashlar | [large_sandstone_blocks_01](https://polyhaven.com/a/large_sandstone_blocks_01) (Poly Haven, Rob Tuytel) | крупные тёсаные блоки известняка |
| rubble | [medieval_blocks_05](https://polyhaven.com/a/medieval_blocks_05) (Poly Haven, Rob Tuytel) | бутовая кладка из рваного известняка на растворе |
| plaster | [worn_mossy_plasterwall](https://polyhaven.com/a/worn_mossy_plasterwall) (Poly Haven, Amal Kumar) | старая известковая штукатурка в потёках |
| roof | своя (Blender, `tools/textures/blender_byz_roof.py`; глина из [clay_roof_tiles_02](https://polyhaven.com/a/clay_roof_tiles_02), Amal Kumar) | черепица VI века: тегулы и калиптеры |
| wood | [Planks039](https://ambientcg.com/view?id=Planks039) | грубые старые доски |
| beam | [Wood060](https://ambientcg.com/view?id=Wood060) | тёмное дерево без швов |
| earth | [Ground109](https://ambientcg.com/view?id=Ground109) | утоптанная сухая земля |
| grass | [Grass004](https://ambientcg.com/view?id=Grass004) | трава |
| soil | [Ground102](https://ambientcg.com/view?id=Ground102) | сухая степная почва |
| rock | [Rock026](https://ambientcg.com/view?id=Rock026) | слоистая скала |
| cobble | [PavingStones138](https://ambientcg.com/view?id=PavingStones138) | булыжная мостовая |
| flagstone | [Tiles143](https://ambientcg.com/view?id=Tiles143) | известняковые плиты пола |
| marble | [Marble014](https://ambientcg.com/view?id=Marble014) | мрамор |
| brick | [Bricks097](https://ambientcg.com/view?id=Bricks097) | старый кирпич (для плинфы) |
| wool | [Fabric045](https://ambientcg.com/view?id=Fabric045) | грубая шерстяная ткань |
| iron | [Metal052C](https://ambientcg.com/view?id=Metal052C) | кованое железо |
| bronze | [Metal047B](https://ambientcg.com/view?id=Metal047B) | бронза/медь с патиной |
| gold | [Metal048C](https://ambientcg.com/view?id=Metal048C) | золото |
| gravel | [Gravel041](https://ambientcg.com/view?id=Gravel041) | береговая галька |
| thatch | своя (Blender, `tools/textures/blender_thatch.py`) | солома и камыш: ряды пучков стеблей внахлёст |
| ceramic | [Clay002](https://ambientcg.com/view?id=Clay002) | обожжённая глина: амфоры, пифосы, горшки |
| leather | [Leather014](https://ambientcg.com/view?id=Leather014) | дублёная кожа |
| linen | [Fabric061](https://ambientcg.com/view?id=Fabric061) | небелёный лён: навесы, паруса, мешки |
| felt | [Fabric034](https://ambientcg.com/view?id=Fabric034) | валяная шерсть: войлок, кошмы |
| rug | [Carpet015](https://ambientcg.com/view?id=Carpet015) | тканый шерстяной ковёр |
| wicker | [Wicker010B](https://ambientcg.com/view?id=Wicker010B) | плетёнка из лозы: корзины |
| limewash | [Plaster003](https://ambientcg.com/view?id=Plaster003) | свежая известковая побелка фасадов |
