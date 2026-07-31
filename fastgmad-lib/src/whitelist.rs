// https://github.com/garrynewman/bootil/blob/beb4cec8ad29533965491b767b177dc549e62d23/src/3rdParty/globber.cpp
// https://github.com/Facepunch/gmad/blob/master/include/AddonWhiteList.h

const ADDON_WHITELIST: &[&str] = &[
    "lua/*.lua",
    "scenes/*.vcd",
    "particles/*.pcf",
    "resource/fonts/*.ttf",
    "scripts/vehicles/*.txt",
    "resource/localization/*/*.properties",
    "maps/*.bsp",
    "maps/*.lmp",
    "maps/*.nav",
    "maps/*.ain",
    "maps/thumb/*.png",
    "sound/*.wav",
    "sound/*.mp3",
    "sound/*.ogg",
    "materials/*.vmt",
    "materials/*.vtf",
    "materials/*.png",
    "materials/*.jpg",
    "materials/*.jpeg",
    "materials/colorcorrection/*.raw",
    "models/*.mdl",
    "models/*.phy",
    "models/*.ani",
    "models/*.vvd",
    "models/*.vtx",
    "!models/*.sw.vtx",
    "!models/*.360.vtx",
    "!models/*.xbox.vtx",
    "gamemodes/*/*.txt",
    "!gamemodes/*/*/*.txt",
    "gamemodes/*/*.fgd",
    "!gamemodes/*/*/*.fgd",
    "gamemodes/*/logo.png",
    "gamemodes/*/icon24.png",
    "gamemodes/*/gamemode/*.lua",
    "gamemodes/*/entities/effects/*.lua",
    "gamemodes/*/entities/weapons/*.lua",
    "gamemodes/*/entities/entities/*.lua",
    "gamemodes/*/backgrounds/*.png",
    "gamemodes/*/backgrounds/*.jpg",
    "gamemodes/*/backgrounds/*.jpeg",
    "gamemodes/*/content/models/*.mdl",
    "gamemodes/*/content/models/*.phy",
    "gamemodes/*/content/models/*.ani",
    "gamemodes/*/content/models/*.vvd",
    "gamemodes/*/content/models/*.vtx",
    "!gamemodes/*/content/models/*.sw.vtx",
    "!gamemodes/*/content/models/*.360.vtx",
    "!gamemodes/*/content/models/*.xbox.vtx",
    "gamemodes/*/content/materials/*.vmt",
    "gamemodes/*/content/materials/*.vtf",
    "gamemodes/*/content/materials/*.png",
    "gamemodes/*/content/materials/*.jpg",
    "gamemodes/*/content/materials/*.jpeg",
    "gamemodes/*/content/materials/colorcorrection/*.raw",
    "gamemodes/*/content/scenes/*.vcd",
    "gamemodes/*/content/particles/*.pcf",
    "gamemodes/*/content/resource/fonts/*.ttf",
    "gamemodes/*/content/scripts/vehicles/*.txt",
    "gamemodes/*/content/resource/localization/*/*.properties",
    "gamemodes/*/content/maps/*.bsp",
    "gamemodes/*/content/maps/*.nav",
    "gamemodes/*/content/maps/*.ain",
    "gamemodes/*/content/maps/thumb/*.png",
    "gamemodes/*/content/sound/*.wav",
    "gamemodes/*/content/sound/*.mp3",
    "gamemodes/*/content/sound/*.ogg",
    "data_static/*.txt",
    "data_static/*.dat",
    "data_static/*.json",
    "data_static/*.xml",
    "data_static/*.csv",
    "shaders/fxc/*.vcs",
];

fn globber(wild: &[u8], path: &[u8]) -> bool {
    let mut w = 0;
    let mut p = 0;
    let mut star_w = usize::MAX;
    let mut star_p = 0;

    while p < path.len() {
        if w < wild.len() && (wild[w] == path[p] || wild[w] == b'?') {
            w += 1;
            p += 1;
        } else if w < wild.len() && wild[w] == b'*' {
            star_w = w;
            star_p = p;
            w += 1;
        } else if star_w != usize::MAX {
            w = star_w + 1;
            star_p += 1;
            p = star_p;
        } else {
            return false;
        }
    }

    while w < wild.len() && wild[w] == b'*' {
        w += 1;
    }

    w == wild.len()
}

pub fn check(path: &str) -> bool {
    ADDON_WHITELIST.iter().any(|glob| globber(glob.as_bytes(), path.as_bytes()))
}

pub fn is_ignored(path: &str, ignore: &[String]) -> bool {
    !ignore.is_empty() && ignore.iter().any(|glob| globber(glob.as_bytes(), path.as_bytes()))
}

#[test]
pub fn test_whitelist() {
    let good = &[
        "lua/test.lua",
        "lua/lol/test.lua",
        "lua/lua/testing.lua",
        "gamemodes/test/something.txt",
        "gamemodes/test/content/sound/lol.wav",
        "materials/lol.jpeg",
        "gamemodes/the_gamemode_name/backgrounds/file_name.jpg",
        "gamemodes/my_base_defence/backgrounds/1.jpg",
    ];
    let bad = &[
        "test.lua",
        "lua/test.exe",
        "lua/lol/test.exe",
        "gamemodes/test",
        "gamemodes/test/something",
        "gamemodes/test/something/something.exe",
        "gamemodes/test/content/sound/lol.vvv",
        "materials/lol.vvv",
    ];

    for good in good { assert!(check(good), "{}", good); }
    for good in ADDON_WHITELIST { assert!(check(&good.replace('*', "test"))); assert!(check(&good.replace('*', "a"))); }
    for bad in bad { assert!(!check(bad)); }
}

#[test]
pub fn test_ignore() {
    assert!(is_ignored("lol.txt", &["lol.txt".to_string()]));
    assert!(is_ignored("lua/hello.lua", &["lua/*.lua".to_string()]));
    assert!(is_ignored("lua/hello.lua", &["lua/*".to_string()]));
    assert!(is_ignored(".gitattributes", &[".git*".to_string()]));
    assert!(!is_ignored("lol.txt", &[]));
}