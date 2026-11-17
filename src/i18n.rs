//! Interface languages. Texts are written in French in the code; `tr` looks up the translation for
//! the chosen language and falls back to French when a text has none.
//!
//! To add a language: add it to `Lang`, `ALL_LANGS`, `Lang::column` and give every row of `TABLE`
//! one more column.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Fr,
    En,
    Es,
    De,
    Zh,
    Ja,
}

pub const ALL_LANGS: [Lang; 6] = [Lang::Fr, Lang::En, Lang::Es, Lang::De, Lang::Zh, Lang::Ja];

impl Lang {
    /// The language's own name, as shown in the menu.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::Zh => "中文（简体）",
            Lang::Ja => "日本語",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
            Lang::De => "de",
            Lang::Zh => "zh",
            Lang::Ja => "ja",
        }
    }

    /// Name stored in the settings, like the original's "language" value (the part of its
    /// "<language>" interface file name before the first ".").
    pub fn setting_name(self) -> &'static str {
        match self {
            Lang::Fr => "french",
            Lang::En => "english",
            Lang::Es => "spanish",
            Lang::De => "german",
            Lang::Zh => "chinese",
            Lang::Ja => "japanese",
        }
    }

    /// From a code ("fr", "en-US", "zh_CN"), a setting name ("french") or a file name ("french.loc").
    pub fn from_code(code: &str) -> Option<Lang> {
        let c = code.trim().to_lowercase();
        let c = c.split('.').next().unwrap_or("");
        ALL_LANGS
            .iter()
            .copied()
            .find(|l| c == l.setting_name())
            .or_else(|| ALL_LANGS.iter().copied().find(|l| c.starts_with(l.code())))
    }

    fn column(self) -> Option<usize> {
        match self {
            Lang::Fr => None,
            Lang::En => Some(0),
            Lang::Es => Some(1),
            Lang::De => Some(2),
            Lang::Zh => Some(3),
            Lang::Ja => Some(4),
        }
    }

    fn index(self) -> u8 {
        ALL_LANGS.iter().position(|l| *l == self).unwrap() as u8
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    ALL_LANGS[CURRENT.load(Ordering::Relaxed) as usize % ALL_LANGS.len()]
}

pub fn set_lang(l: Lang) {
    CURRENT.store(l.index(), Ordering::Relaxed);
}

/// Translate a French interface text.
///
/// Like the original (the reference loads the English interface file first, then lays the chosen language
/// over it), English is the base: a text the chosen language lacks is shown in English, and only a
/// text with no row at all stays as written in the code.
/// Translate a French interface text. A key may end in `§context` (`"Image§menu"`) when the
/// same French word needs another translation somewhere; French shows the part before `§`.
pub fn tr(fr: &'static str) -> &'static str {
    let base = fr.split('§').next().unwrap_or(fr);
    let Some(col) = lang().column() else { return base };
    match map().get(fr) {
        Some(row) if !row[col].is_empty() => row[col],
        Some(row) if !row[0].is_empty() => row[0],
        _ => base,
    }
}

/// Translate a French template with `{}` placeholders, then fill them in order.
pub fn trf(fr: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let template = tr(fr);
    let mut out = String::with_capacity(template.len() + 16);
    let mut parts = template.split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for (i, part) in parts.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(part);
    }
    out
}

fn map() -> &'static HashMap<&'static str, [&'static str; 5]> {
    static MAP: OnceLock<HashMap<&'static str, [&'static str; 5]>> = OnceLock::new();
    MAP.get_or_init(|| TABLE.iter().map(|(k, v)| (*k, *v)).collect())
}

/// The original's default when nothing is chosen ("english").
pub const DEFAULT_LANG: Lang = Lang::En;

/// Language from the computer's settings (our addition), else the original's default, English.
pub fn system_lang() -> Lang {
    sys_locale::get_locale().and_then(|l| Lang::from_code(&l)).unwrap_or(DEFAULT_LANG)
}

/// Folder for this program's own settings (language, tool presets).
pub fn config_dir() -> Option<std::path::PathBuf> {
    // a chosen folder (portable use, tests)
    if let Some(d) = std::env::var_os("TOONLIGHT_CONFIG_DIR").or_else(|| std::env::var_os("ANIMCRAFT_CONFIG_DIR")) {
        return Some(std::path::PathBuf::from(d));
    }
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let base = base?;
    let dir = base.join("toonlight");
    // the app was called AnimCraft before: its settings folder moves to the new name once
    let old = base.join("animcraft");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}

fn settings_file() -> Option<std::path::PathBuf> {
    config_dir().map(|d| d.join("language"))
}

static OVERRIDE: OnceLock<Lang> = OnceLock::new();

/// Language given on the command line (`--lang en`), used instead of the saved one.
pub fn set_override(l: Lang) {
    let _ = OVERRIDE.set(l);
}

/// The language from the command line, else the one chosen last time, else the computer's.
pub fn load_lang() -> Lang {
    if let Some(l) = OVERRIDE.get() {
        return *l;
    }
    settings_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| Lang::from_code(&s))
        .unwrap_or_else(system_lang)
}

pub fn save_lang(l: Lang) {
    if let Some(p) = settings_file() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, l.setting_name());
    }
}

/// A font file on this computer with Chinese and Japanese characters, if there is one.
pub fn find_cjk_font(prefer_japanese: bool) -> Option<(std::path::PathBuf, u32)> {
    let ja: &[(&str, u32)] = &[
        ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
        ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
        ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
    ];
    let common: &[(&str, u32)] = &[
        ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
        ("/Library/Fonts/Arial Unicode.ttf", 0),
        ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
        ("/System/Library/Fonts/PingFang.ttc", 0),
        ("/System/Library/Fonts/STHeiti Light.ttc", 0),
        ("C:\\Windows\\Fonts\\msyh.ttc", 0),
        ("C:\\Windows\\Fonts\\simhei.ttf", 0),
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
        ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
        ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 0),
        ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", 0),
        ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
    ];
    let order: Vec<&(&str, u32)> = if prefer_japanese {
        ja.iter().chain(common).collect()
    } else {
        common.iter().chain(ja).collect()
    };
    order
        .into_iter()
        .map(|(p, i)| (std::path::PathBuf::from(p), *i))
        .find(|(p, _)| p.is_file())
}

/// French text → English, Spanish, German, Chinese (simplified), Japanese.
#[rustfmt::skip]
static TABLE: &[(&str, [&str; 5])] = &[
    // themes and start
    ("Orage (sombre)", ["Storm (dark)", "Tormenta (oscuro)", "Gewitter (dunkel)", "风暴（深色）", "嵐（ダーク）"]),
    ("Acier (clair)", ["Steel (light)", "Acero (claro)", "Stahl (hell)", "钢（浅色）", "スチール（ライト）"]),
    ("Exemple chargé : une balle qui rebondit. Fichier > Nouveau pour une page blanche.", ["Example loaded: a bouncing ball. File > New for a blank page.", "Ejemplo cargado: una pelota que rebota. Archivo > Nuevo para una página en blanco.", "Beispiel geladen: ein hüpfender Ball. Datei > Neu für eine leere Seite.", "已加载示例：弹跳的球。选择 文件 > 新建 得到空白页面。", "サンプルを読み込みました：弾むボール。白紙は ファイル > 新規 から。"]),
    ("Décor", ["Background", "Fondo", "Hintergrund", "背景", "背景"]),
    ("Balle", ["Ball", "Pelota", "Ball", "球", "ボール"]),
    ("{} copie", ["{} copy", "{} copia", "{} Kopie", "{} 副本", "{} コピー"]),
    // status messages
    ("Projet ouvert : {}", ["Project opened: {}", "Proyecto abierto: {}", "Projekt geöffnet: {}", "已打开项目：{}", "プロジェクトを開きました：{}"]),
    ("Impossible d'ouvrir le projet : {}", ["Cannot open the project: {}", "No se puede abrir el proyecto: {}", "Projekt kann nicht geöffnet werden: {}", "无法打开项目：{}", "プロジェクトを開けません：{}"]),
    ("Projet d'animation", ["Animation project", "Proyecto de animación", "Animationsprojekt", "动画项目", "アニメーションプロジェクト"]),
    ("Enregistré : {}", ["Saved: {}", "Guardado: {}", "Gespeichert: {}", "已保存：{}", "保存しました：{}"]),
    ("Échec de l'enregistrement : {}", ["Save failed: {}", "Error al guardar: {}", "Speichern fehlgeschlagen: {}", "保存失败：{}", "保存に失敗しました：{}"]),
    ("{} images exportées dans {}", ["{} frames exported to {}", "{} imágenes exportadas en {}", "{} Bilder exportiert nach {}", "已导出 {} 帧到 {}", "{} 枚の画像を書き出しました：{}"]),
    ("Échec de l'export : {}", ["Export failed: {}", "Error al exportar: {}", "Export fehlgeschlagen: {}", "导出失败：{}", "書き出しに失敗しました：{}"]),
    ("Dossier d'export (JSON + images)", ["Export folder (JSON + images)", "Carpeta de exportación (JSON + imágenes)", "Exportordner (JSON + Bilder)", "导出文件夹（JSON + 图像）", "書き出しフォルダー（JSON + 画像）"]),
    ("Structure exportée pour After Effects : {}", ["Structure exported for After Effects: {}", "Estructura exportada para After Effects: {}", "Struktur für After Effects exportiert: {}", "已为 After Effects 导出结构：{}", "After Effects 用に構造を書き出しました：{}"]),
    ("GIF animé", ["Animated GIF", "GIF animado", "Animiertes GIF", "GIF 动画", "アニメーションGIF"]),
    ("GIF exporté : {}", ["GIF exported: {}", "GIF exportado: {}", "GIF exportiert: {}", "已导出 GIF：{}", "GIFを書き出しました：{}"]),
    ("L'export vidéo a besoin de FFmpeg (gratuit, ffmpeg.org) installé sur l'ordinateur.", ["Video export needs FFmpeg (free, ffmpeg.org) installed on the computer.", "La exportación de vídeo necesita FFmpeg (gratuito, ffmpeg.org) instalado en el ordenador.", "Der Videoexport benötigt FFmpeg (kostenlos, ffmpeg.org) auf dem Computer.", "导出视频需要在电脑上安装 FFmpeg（免费，ffmpeg.org）。", "動画の書き出しには FFmpeg（無料、ffmpeg.org）のインストールが必要です。"]),
    ("Vidéo MP4", ["MP4 video", "Vídeo MP4", "MP4-Video", "MP4 视频", "MP4動画"]),
    ("Vidéo exportée : {}", ["Video exported: {}", "Vídeo exportado: {}", "Video exportiert: {}", "已导出视频：{}", "動画を書き出しました：{}"]),
    ("Échec de l'export vidéo : {}", ["Video export failed: {}", "Error al exportar el vídeo: {}", "Videoexport fehlgeschlagen: {}", "视频导出失败：{}", "動画の書き出しに失敗しました：{}"]),
    ("Ce calque est verrouillé.", ["This layer is locked.", "Esta capa está bloqueada.", "Diese Ebene ist gesperrt.", "此图层已锁定。", "このレイヤーはロックされています。"]),
    ("« {} » est verrouillé", ["“{}” is locked", "«{}» está bloqueada", "„{}“ ist gesperrt", "“{}”已锁定", "「{}」はロックされています"]),
    ("Nouveau projet {} × {}", ["New project {} × {}", "Nuevo proyecto {} × {}", "Neues Projekt {} × {}", "新建项目 {} × {}", "新規プロジェクト {} × {}"]),
    ("Ce fichier n'est pas un projet d'animation.", ["This file is not an animation project.", "Este archivo no es un proyecto de animación.", "Diese Datei ist kein Animationsprojekt.", "此文件不是动画项目。", "このファイルはアニメーションプロジェクトではありません。"]),
    ("Une image du projet n'a pas la bonne taille.", ["A picture in the project has the wrong size.", "Una imagen del proyecto no tiene el tamaño correcto.", "Ein Bild im Projekt hat die falsche Größe.", "项目中的某个图像尺寸不正确。", "プロジェクト内の画像のサイズが正しくありません。"]),
    ("Le projet n'a aucun calque.", ["The project has no layers.", "El proyecto no tiene capas.", "Das Projekt hat keine Ebenen.", "项目没有图层。", "プロジェクトにレイヤーがありません。"]),
    // menus
    ("Fichier", ["File", "Archivo", "Datei", "文件", "ファイル"]),
    ("Nouveau projet…", ["New project…", "Nuevo proyecto…", "Neues Projekt…", "新建项目…", "新規プロジェクト…"]),
    ("Ouvrir…", ["Open…", "Abrir…", "Öffnen…", "打开…", "開く…"]),
    ("Enregistrer", ["Save", "Guardar", "Speichern", "保存", "保存"]),
    ("Enregistrer sous…", ["Save as…", "Guardar como…", "Speichern unter…", "另存为…", "名前を付けて保存…"]),
    ("Exporter", ["Export", "Exportar", "Exportieren", "导出", "書き出し"]),
    ("Suite d'images PNG…", ["PNG image sequence…", "Secuencia de imágenes PNG…", "PNG-Bildfolge…", "PNG 图像序列…", "PNG連番画像…"]),
    ("GIF animé…", ["Animated GIF…", "GIF animado…", "Animiertes GIF…", "GIF 动画…", "アニメーションGIF…"]),
    ("Vidéo MP4…", ["MP4 video…", "Vídeo MP4…", "MP4-Video…", "MP4 视频…", "MP4動画…"]),
    ("Avec FFmpeg", ["With FFmpeg", "Con FFmpeg", "Mit FFmpeg", "使用 FFmpeg", "FFmpeg を使用"]),
    ("Nécessite FFmpeg (ffmpeg.org)", ["Needs FFmpeg (ffmpeg.org)", "Requiere FFmpeg (ffmpeg.org)", "Benötigt FFmpeg (ffmpeg.org)", "需要 FFmpeg（ffmpeg.org）", "FFmpeg が必要です（ffmpeg.org）"]),
    ("Structure des calques pour After Effects (JSON + images)…", ["Layer structure for After Effects (JSON + images)…", "Estructura de capas para After Effects (JSON + imágenes)…", "Ebenenstruktur für After Effects (JSON + Bilder)…", "用于 After Effects 的图层结构（JSON + 图像）…", "After Effects 用レイヤー構造（JSON + 画像）…"]),
    ("Lisible par le script d'import After Effects (JSON)", ["Readable by the After Effects JSON import script", "Legible por el script de importación JSON de After Effects", "Lesbar für das JSON-Importskript von After Effects", "可由 After Effects JSON 导入脚本读取", "After Effects の JSON 読み込みスクリプトで読めます"]),
    ("Quitter", ["Quit", "Salir", "Beenden", "退出", "終了"]),
    ("Édition", ["Edit", "Edición", "Bearbeiten", "编辑", "編集"]),
    ("Annuler", ["Undo", "Deshacer", "Rückgängig", "撤销", "元に戻す"]),
    ("Rétablir", ["Redo", "Rehacer", "Wiederholen", "重做", "やり直す"]),
    ("Effacer le dessin", ["Clear drawing", "Borrar el dibujo", "Zeichnung löschen", "清除画面", "絵を消去"]),
    ("Animation", ["Animation", "Animación", "Animation", "动画", "アニメーション"]),
    ("Arrêter", ["Stop", "Detener", "Stopp", "停止", "停止"]),
    ("Lire", ["Play", "Reproducir", "Abspielen", "播放", "再生"]),
    ("Lecture en boucle", ["Loop playback", "Reproducción en bucle", "Endlos abspielen", "循环播放", "ループ再生"]),
    ("Nouvelle image après", ["New frame after", "Nueva imagen después", "Neues Bild danach", "在后面新建帧", "後ろに新しい画像"]),
    ("Dupliquer l'image", ["Duplicate frame", "Duplicar imagen", "Bild duplizieren", "复制帧", "画像を複製"]),
    ("Nouveau calque", ["New layer", "Nueva capa", "Neue Ebene", "新建图层", "新規レイヤー"]),
    ("Affichage", ["View", "Ver", "Ansicht", "视图", "表示"]),
    ("Ajuster à la fenêtre", ["Fit to window", "Ajustar a la ventana", "An Fenster anpassen", "适合窗口", "ウィンドウに合わせる"]),
    ("Taille réelle (100 %)", ["Actual size (100%)", "Tamaño real (100 %)", "Originalgröße (100 %)", "实际大小（100%）", "実寸（100%）"]),
    ("Tant que cette fenêtre est ouverte, l'IA travaille en direct sur le projet ouvert ici (chaque changement s'annule avec Annuler). Fenêtre fermée, elle travaille sur son propre projet.", ["While this window is open, the AI works live on the project open here (each change can be undone). With the window closed, it works on its own project.", "Mientras esta ventana está abierta, la IA trabaja en directo en el proyecto abierto aquí (cada cambio se puede deshacer). Con la ventana cerrada, trabaja en su propio proyecto.", "Solange dieses Fenster offen ist, arbeitet die KI live am hier geöffneten Projekt (jede Änderung lässt sich rückgängig machen). Bei geschlossenem Fenster arbeitet sie an ihrem eigenen Projekt.", "此窗口打开时，AI 会实时处理这里打开的项目（每个更改都可撤销）。窗口关闭时，它处理自己的项目。", "このウィンドウが開いている間、AI はここで開いているプロジェクトを直接操作します（変更は取り消せます）。閉じている時は自分のプロジェクトを使います。"]),
    ("Une IA a modifié le projet (Annuler pour revenir en arrière).", ["An AI changed the project (Undo to go back).", "Una IA modificó el proyecto (Deshacer para volver atrás).", "Eine KI hat das Projekt geändert (Rückgängig zum Zurücknehmen).", "AI 修改了项目（撤销可返回）。", "AI がプロジェクトを変更しました（取り消しで戻せます）。"]),
    ("Effets du calque (pile)…", ["Layer effects (stack)…", "Efectos de la capa (pila)…", "Ebeneneffekte (Stapel)…", "图层效果（堆栈）…", "レイヤー効果（スタック）…"]),
    ("Effets du calque « {} »", ["Effects of layer “{}”", "Efectos de la capa «{}»", "Effekte der Ebene „{}“", "图层“{}”的效果", "レイヤー「{}」の効果"]),
    ("Les effets s'appliquent dans l'ordre à chaque image du calque, sans changer les dessins.", ["The effects apply in order to every frame of the layer, without changing the drawings.", "Los efectos se aplican en orden a cada imagen de la capa, sin cambiar los dibujos.", "Die Effekte wirken der Reihe nach auf jedes Bild der Ebene, ohne die Zeichnungen zu ändern.", "效果按顺序作用于图层的每一帧，不改变画本身。", "効果は順にレイヤーの各フレームにかかり、絵そのものは変わりません。"]),
    ("+ Ajouter", ["+ Add", "+ Añadir", "+ Hinzufügen", "+ 添加", "+ 追加"]),
    ("Actif", ["On", "Activo", "An", "启用", "有効"]),
    ("Clés aux images : {}", ["Keys at frames: {}", "Claves en las imágenes: {}", "Schlüssel bei Bildern: {}", "关键帧：{}", "キーのフレーム：{}"]),
    ("Appliquer aux dessins", ["Apply to the drawings", "Aplicar a los dibujos", "Auf die Zeichnungen anwenden", "应用到画上", "絵に適用"]),
    ("Peint les effets dans les dessins pour de bon et vide la pile", ["Paints the effects into the drawings for good and empties the stack", "Pinta los efectos en los dibujos para siempre y vacía la pila", "Malt die Effekte dauerhaft in die Zeichnungen und leert den Stapel", "将效果永久画入画中并清空堆栈", "効果を絵に確定し、スタックを空にします"]),
    ("Tout supprimer", ["Remove all", "Quitar todo", "Alle entfernen", "全部删除", "すべて削除"]),
    ("Aucun effet actif à appliquer, ou le calque est verrouillé.", ["No effect on to apply, or the layer is locked.", "Ningún efecto activo que aplicar, o la capa está bloqueada.", "Kein aktiver Effekt zum Anwenden, oder die Ebene ist gesperrt.", "没有可应用的启用效果，或图层已锁定。", "適用する有効な効果がないか、レイヤーがロックされています。"]),
    ("Verrous :", ["Locks:", "Bloqueos:", "Sperren:", "锁定：", "ロック："]),
    ("Copier le calque", ["Copy layer", "Copiar la capa", "Ebene kopieren", "复制图层", "レイヤーをコピー"]),
    ("Couper le calque", ["Cut layer", "Cortar la capa", "Ebene ausschneiden", "剪切图层", "レイヤーを切り取り"]),
    ("Coller le calque", ["Paste layer", "Pegar la capa", "Ebene einfügen", "粘贴图层", "レイヤーを貼り付け"]),
    ("Calque copié.", ["Layer copied.", "Capa copiada.", "Ebene kopiert.", "图层已复制。", "レイヤーをコピーしました。"]),
    ("Étirer à", ["Stretch to", "Estirar a", "Strecken auf", "拉伸到", "伸縮："]),
    ("Étirer le calque", ["Stretch layer", "Estirar la capa", "Ebene strecken", "拉伸图层", "レイヤーを伸縮"]),
    ("Rien à étirer sur ce calque.", ["Nothing to stretch on this layer.", "Nada que estirar en esta capa.", "Nichts zu strecken auf dieser Ebene.", "此图层没有可拉伸的内容。", "このレイヤーには伸縮するものがありません。"]),
    ("Pression", ["Pressure", "Presión", "Druck", "压力", "筆圧"]),
    ("Vitesse", ["Speed", "Velocidad", "Geschwindigkeit", "速度", "速度"]),
    ("Hasard", ["Random", "Azar", "Zufall", "随机", "ランダム"]),
    ("Direction", ["Direction", "Dirección", "Richtung", "方向", "方向"]),
    ("Connexions (ce qui fait varier le trait) :", ["Connections (what changes the stroke):", "Conexiones (lo que hace variar el trazo):", "Verbindungen (was den Strich verändert):", "关联（改变笔触的因素）：", "連動（線を変化させるもの）："]),
    ("Inv.", ["Inv.", "Inv.", "Inv.", "反", "反転"]),
    ("Inverser : plus la pression est forte, plus la valeur baisse", ["Invert: the stronger the input, the lower the value", "Invertir: cuanto más fuerte, menor el valor", "Umkehren: je stärker, desto kleiner der Wert", "反转：输入越强，数值越低", "反転：入力が強いほど値が下がる"]),
    ("Courbe : négative = doux, positive = dur", ["Curve: negative = soft, positive = hard", "Curva: negativa = suave, positiva = dura", "Kurve: negativ = weich, positiv = hart", "曲线：负数为柔和，正数为硬", "カーブ：マイナスで柔らかく、プラスで硬く"]),
    ("Double-clic : taper le numéro du dessin (« - » pour tenir)", ["Double-click: type the drawing number (\"-\" to hold)", "Doble clic: escriba el número del dibujo («-» para mantener)", "Doppelklick: Nummer der Zeichnung eingeben („-“ zum Halten)", "双击：输入画的编号（“-” 表示保持）", "ダブルクリック：絵の番号を入力（「-」で保持）"]),
    ("Tapez un numéro de dessin, ou « - » pour tenir le dessin d'avant.", ["Type a drawing number, or \"-\" to hold the drawing before.", "Escriba un número de dibujo, o «-» para mantener el dibujo anterior.", "Geben Sie eine Zeichnungsnummer ein oder „-“, um die vorige Zeichnung zu halten.", "请输入画的编号，或输入“-”保持前一张画。", "絵の番号を入力するか、「-」で前の絵を保持します。"]),
    ("Premier plan", ["Foreground", "Primer plano", "Vordergrund", "前景", "前景"]),
    ("Trait", ["Line", "Línea", "Linie", "线稿", "線画"]),
    ("Ombre", ["Shadow", "Sombra", "Schatten", "阴影", "影"]),
    ("Dessin animé simple 2 s", ["Basic cartoon 2 s", "Dibujo animado simple 2 s", "Einfacher Zeichentrick 2 s", "基础动画 2 秒", "シンプルなアニメ 2 秒"]),
    ("Dessin animé simple 5 s", ["Basic cartoon 5 s", "Dibujo animado simple 5 s", "Einfacher Zeichentrick 5 s", "基础动画 5 秒", "シンプルなアニメ 5 秒"]),
    ("Dessin animé simple 10 s", ["Basic cartoon 10 s", "Dibujo animado simple 10 s", "Einfacher Zeichentrick 10 s", "基础动画 10 秒", "シンプルなアニメ 10 秒"]),
    ("Film en doublé 2 s, 2 calques", ["Film on twos 2 s, 2 layers", "Película en dobles 2 s, 2 capas", "Film auf Zweien 2 s, 2 Ebenen", "电影一拍二 2 秒，2 个图层", "映画 2 コマ打ち 2 秒、2 レイヤー"]),
    ("Film en doublé 5 s, 2 calques", ["Film on twos 5 s, 2 layers", "Película en dobles 5 s, 2 capas", "Film auf Zweien 5 s, 2 Ebenen", "电影一拍二 5 秒，2 个图层", "映画 2 コマ打ち 5 秒、2 レイヤー"]),
    ("Film en doublé 10 s, 2 calques", ["Film on twos 10 s, 2 layers", "Película en dobles 10 s, 2 capas", "Film auf Zweien 10 s, 2 Ebenen", "电影一拍二 10 秒，2 个图层", "映画 2 コマ打ち 10 秒、2 レイヤー"]),
    ("Film en doublé 2 s, 5 calques", ["Film on twos 2 s, 5 layers", "Película en dobles 2 s, 5 capas", "Film auf Zweien 2 s, 5 Ebenen", "电影一拍二 2 秒，5 个图层", "映画 2 コマ打ち 2 秒、5 レイヤー"]),
    ("Film en doublé 5 s, 5 calques", ["Film on twos 5 s, 5 layers", "Película en dobles 5 s, 5 capas", "Film auf Zweien 5 s, 5 Ebenen", "电影一拍二 5 秒，5 个图层", "映画 2 コマ打ち 5 秒、5 レイヤー"]),
    ("Film en doublé 10 s, 5 calques", ["Film on twos 10 s, 5 layers", "Película en dobles 10 s, 5 capas", "Film auf Zweien 10 s, 5 Ebenen", "电影一拍二 10 秒，5 个图层", "映画 2 コマ打ち 10 秒、5 レイヤー"]),
    ("TV PAL 16:9 en doublé 2 s, 2 calques", ["TV PAL 16:9 on twos 2 s, 2 layers", "TV PAL 16:9 en dobles 2 s, 2 capas", "TV PAL 16:9 auf Zweien 2 s, 2 Ebenen", "电视 PAL 16:9 一拍二 2 秒，2 个图层", "TV PAL 16:9 2 コマ打ち 2 秒、2 レイヤー"]),
    ("TV PAL 16:9 en doublé 10 s, 2 calques", ["TV PAL 16:9 on twos 10 s, 2 layers", "TV PAL 16:9 en dobles 10 s, 2 capas", "TV PAL 16:9 auf Zweien 10 s, 2 Ebenen", "电视 PAL 16:9 一拍二 10 秒，2 个图层", "TV PAL 16:9 2 コマ打ち 10 秒、2 レイヤー"]),
    ("TV PAL 16:9 en doublé 2 s, 5 calques", ["TV PAL 16:9 on twos 2 s, 5 layers", "TV PAL 16:9 en dobles 2 s, 5 capas", "TV PAL 16:9 auf Zweien 2 s, 5 Ebenen", "电视 PAL 16:9 一拍二 2 秒，5 个图层", "TV PAL 16:9 2 コマ打ち 2 秒、5 レイヤー"]),
    ("TV PAL 16:9 en doublé 10 s, 5 calques", ["TV PAL 16:9 on twos 10 s, 5 layers", "TV PAL 16:9 en dobles 10 s, 5 capas", "TV PAL 16:9 auf Zweien 10 s, 5 Ebenen", "电视 PAL 16:9 一拍二 10 秒，5 个图层", "TV PAL 16:9 2 コマ打ち 10 秒、5 レイヤー"]),
    ("TV PAL 4:3 en doublé 2 s, 2 calques", ["TV PAL 4:3 on twos 2 s, 2 layers", "TV PAL 4:3 en dobles 2 s, 2 capas", "TV PAL 4:3 auf Zweien 2 s, 2 Ebenen", "电视 PAL 4:3 一拍二 2 秒，2 个图层", "TV PAL 4:3 2 コマ打ち 2 秒、2 レイヤー"]),
    ("TV PAL 4:3 en doublé 10 s, 2 calques", ["TV PAL 4:3 on twos 10 s, 2 layers", "TV PAL 4:3 en dobles 10 s, 2 capas", "TV PAL 4:3 auf Zweien 10 s, 2 Ebenen", "电视 PAL 4:3 一拍二 10 秒，2 个图层", "TV PAL 4:3 2 コマ打ち 10 秒、2 レイヤー"]),
    ("TV PAL 4:3 en doublé 2 s, 5 calques", ["TV PAL 4:3 on twos 2 s, 5 layers", "TV PAL 4:3 en dobles 2 s, 5 capas", "TV PAL 4:3 auf Zweien 2 s, 5 Ebenen", "电视 PAL 4:3 一拍二 2 秒，5 个图层", "TV PAL 4:3 2 コマ打ち 2 秒、5 レイヤー"]),
    ("TV PAL 4:3 en doublé 10 s, 5 calques", ["TV PAL 4:3 on twos 10 s, 5 layers", "TV PAL 4:3 en dobles 10 s, 5 capas", "TV PAL 4:3 auf Zweien 10 s, 5 Ebenen", "电视 PAL 4:3 一拍二 10 秒，5 个图层", "TV PAL 4:3 2 コマ打ち 10 秒、5 レイヤー"]),
    ("Bannière web 10 i/s", ["Web banner 10 fps", "Banner web 10 i/s", "Web-Banner 10 B/s", "网页横幅 10 帧/秒", "Web バナー 10 fps"]),
    ("Bannière web 15 i/s", ["Web banner 15 fps", "Banner web 15 i/s", "Web-Banner 15 B/s", "网页横幅 15 帧/秒", "Web バナー 15 fps"]),
    ("Modèle", ["Template", "Plantilla", "Vorlage", "模板", "テンプレート"]),
    ("Aucun (un calque)", ["None (one layer)", "Ninguna (una capa)", "Keine (eine Ebene)", "无（一个图层）", "なし（1 レイヤー）"]),
    ("fois :", ["times:", "veces:", "Mal:", "次：", "回："]),
    ("en boucle", ["loop", "en bucle", "Schleife", "循环", "ループ"]),
    ("aller-retour", ["ping-pong", "ida y vuelta", "hin und zurück", "往返", "往復"]),
    ("au hasard", ["random", "al azar", "zufällig", "随机", "ランダム"]),
    ("Historique d'annulation…", ["Undo history…", "Historial de deshacer…", "Rückgängig-Verlauf…", "撤销历史…", "取り消し履歴…"]),
    ("Historique d'annulation", ["Undo history", "Historial de deshacer", "Rückgängig-Verlauf", "撤销历史", "取り消し履歴"]),
    ("Cliquez une étape pour revenir à cet état.", ["Click a step to go back to that state.", "Haga clic en un paso para volver a ese estado.", "Klicken Sie auf einen Schritt, um zu diesem Zustand zurückzukehren.", "点击某一步以回到该状态。", "手順をクリックするとその状態に戻ります。"]),
    ("Début", ["Start", "Inicio", "Anfang", "开始", "最初"]),
    ("Dessin", ["Drawing", "Dibujo", "Zeichnung", "绘画", "描画"]),
    ("Calques et images", ["Layers and frames", "Capas e imágenes", "Ebenen und Bilder", "图层和帧", "レイヤーとフレーム"]),
    ("Suite d'images dans un autre format", ["Picture sequence in another format", "Secuencia de imágenes en otro formato", "Bildfolge in einem anderen Format", "其他格式的图片序列", "別形式の連番画像"]),
    ("Exporter en page web (HTML)…", ["Export as a web page (HTML)…", "Exportar como página web (HTML)…", "Als Webseite exportieren (HTML)…", "导出为网页（HTML）…", "Web ページとして書き出す（HTML）…"]),
    ("Une page avec une vignette, la durée et les notes de chaque plan ; imprimable en PDF depuis le navigateur.", ["A page with a picture, the length and the notes of each shot; print it to PDF from the browser.", "Una página con una miniatura, la duración y las notas de cada plano; imprimible en PDF desde el navegador.", "Eine Seite mit Bild, Länge und Notizen jeder Einstellung; im Browser als PDF druckbar.", "每个镜头一张缩略图、时长和备注的页面；可在浏览器中打印为 PDF。", "各カットの画像・長さ・メモを並べたページ。ブラウザから PDF に印刷できます。"]),
    ("Storyboard exporté : {}", ["Storyboard exported: {}", "Storyboard exportado: {}", "Storyboard exportiert: {}", "故事板已导出：{}", "絵コンテを書き出しました：{}"]),
    ("{} images", ["{} frames", "{} imágenes", "{} Bilder", "{} 帧", "{} フレーム"]),
    ("Image PSD avec ses calques (image courante)…", ["PSD picture with its layers (current frame)…", "Imagen PSD con sus capas (imagen actual)…", "PSD-Bild mit Ebenen (aktuelles Bild)…", "带图层的 PSD 图片（当前帧）…", "レイヤー付き PSD 画像（現在のフレーム）…"]),
    ("Importer un PSD (ses calques deviennent des calques)…", ["Import a PSD (its layers become layers)…", "Importar un PSD (sus capas se vuelven capas)…", "PSD importieren (Ebenen werden Ebenen)…", "导入 PSD（其图层成为图层）…", "PSD を読み込む（レイヤーをそのまま）…"]),
    ("Exporté : {}", ["Exported: {}", "Exportado: {}", "Exportiert: {}", "已导出：{}", "書き出しました：{}"]),
    ("{} calques importés.", ["{} layers imported.", "{} capas importadas.", "{} Ebenen importiert.", "已导入 {} 个图层。", "{} 個のレイヤーを読み込みました。"]),
    ("Ce PSD n'a pas de calques.", ["This PSD has no layers.", "Este PSD no tiene capas.", "Dieses PSD hat keine Ebenen.", "此 PSD 没有图层。", "この PSD にはレイヤーがありません。"]),
    ("Fichier PSD incomplet.", ["Incomplete PSD file.", "Archivo PSD incompleto.", "Unvollständige PSD-Datei.", "PSD 文件不完整。", "PSD ファイルが不完全です。"]),
    ("Ce fichier n'est pas un PSD lisible (PSD version 1 seulement).", ["This file is not a readable PSD (PSD version 1 only).", "Este archivo no es un PSD legible (solo PSD versión 1).", "Diese Datei ist kein lesbares PSD (nur PSD Version 1).", "此文件不是可读的 PSD（仅支持 PSD 版本 1）。", "読み込めない PSD です（PSD バージョン 1 のみ）。"]),
    ("Seuls les PSD RVB 8 bits sont lus.", ["Only 8-bit RGB PSD files are read.", "Solo se leen PSD RGB de 8 bits.", "Nur 8-Bit-RGB-PSD werden gelesen.", "仅支持 8 位 RGB 的 PSD。", "8 ビット RGB の PSD のみ読み込めます。"]),
    ("Compression PSD non prise en charge (seulement brut ou PackBits).", ["PSD compression not supported (raw or PackBits only).", "Compresión PSD no admitida (solo sin comprimir o PackBits).", "PSD-Komprimierung nicht unterstützt (nur roh oder PackBits).", "不支持此 PSD 压缩（仅支持无压缩或 PackBits）。", "この PSD の圧縮には対応していません（無圧縮または PackBits のみ）。"]),
    ("Importer une vidéo nécessite FFmpeg (ffmpeg.org).", ["Importing a video needs FFmpeg (ffmpeg.org).", "Importar un vídeo necesita FFmpeg (ffmpeg.org).", "Für den Videoimport wird FFmpeg benötigt (ffmpeg.org).", "导入视频需要 FFmpeg（ffmpeg.org）。", "動画の読み込みには FFmpeg が必要です（ffmpeg.org）。"]),
    ("Vidéo", ["Video", "Vídeo", "Video", "视频", "動画"]),
    ("Échec de l'import : {}", ["Import failed: {}", "Error al importar: {}", "Import fehlgeschlagen: {}", "导入失败：{}", "読み込みに失敗しました：{}"]),
    ("Aucune image à importer.", ["No picture to import.", "Ninguna imagen para importar.", "Kein Bild zum Importieren.", "没有可导入的图片。", "読み込む画像がありません。"]),
    ("Retourner horizontalement", ["Flip horizontally", "Voltear horizontalmente", "Horizontal spiegeln", "水平翻转", "左右反転"]),
    ("Retourner verticalement", ["Flip vertically", "Voltear verticalmente", "Vertikal spiegeln", "垂直翻转", "上下反転"]),
    ("Importer une vidéo (nouveau calque)…", ["Import a video (new layer)…", "Importar un vídeo (capa nueva)…", "Video importieren (neue Ebene)…", "导入视频（新图层）…", "動画を読み込む（新規レイヤー）…"]),
    ("Image collée depuis le presse-papiers.", ["Picture pasted from the clipboard.", "Imagen pegada desde el portapapeles.", "Bild aus der Zwischenablage eingefügt.", "已从剪贴板粘贴图片。", "クリップボードから画像を貼り付けました。"]),
    ("Travail non enregistré retrouvé", ["Unsaved work found", "Trabajo sin guardar encontrado", "Nicht gespeicherte Arbeit gefunden", "找到未保存的工作", "保存されていない作業が見つかりました"]),
    ("Toonlight s'est fermé sans enregistrer. Une copie de votre travail a été gardée.", ["Toonlight closed without saving. A copy of your work was kept.", "Toonlight se cerró sin guardar. Se guardó una copia de su trabajo.", "Toonlight wurde ohne Speichern geschlossen. Eine Kopie Ihrer Arbeit wurde aufbewahrt.", "Toonlight 未保存就关闭了。已保留您工作的副本。", "Toonlight が保存せずに終了しました。作業のコピーが残っています。"]),
    ("Rouvrir la copie", ["Open the copy", "Abrir la copia", "Kopie öffnen", "打开副本", "コピーを開く"]),
    ("Ignorer", ["Ignore", "Ignorar", "Ignorieren", "忽略", "無視"]),
    ("Enregistrement", ["Saving", "Guardado", "Speichern", "保存", "保存"]),
    ("Garder une copie de la version précédente à chaque enregistrement", ["Keep a copy of the previous version at each save", "Guardar una copia de la versión anterior en cada guardado", "Bei jedem Speichern eine Kopie der vorigen Version behalten", "每次保存时保留上一版本的副本", "保存のたびに前のバージョンのコピーを残す"]),
    ("Dossier des copies :", ["Copies folder:", "Carpeta de copias:", "Ordner der Kopien:", "副本文件夹：", "コピーの保存先："]),
    ("à côté du projet", ["next to the project", "junto al proyecto", "neben dem Projekt", "项目旁边", "プロジェクトと同じ場所"]),
    ("Choisir…", ["Choose…", "Elegir…", "Wählen…", "选择…", "選択…"]),
    ("À côté du projet", ["Next to the project", "Junto al proyecto", "Neben dem Projekt", "项目旁边", "プロジェクトと同じ場所"]),
    ("Copie de secours toutes les", ["Recovery copy every", "Copia de seguridad cada", "Sicherungskopie alle", "每隔", "復旧用コピーの間隔"]),
    ("minutes (0 = jamais)", ["minutes (0 = never)", "minutos (0 = nunca)", "Minuten (0 = nie)", "分钟保存恢复副本（0 = 从不）", "分（0 = しない）"]),
    ("Mémoire", ["Memory", "Memoria", "Speicher", "内存", "メモリ"]),
    ("Annulation :", ["Undo:", "Deshacer:", "Rückgängig:", "撤销：", "取り消し："]),
    ("Préférences…", ["Preferences…", "Preferencias…", "Einstellungen…", "首选项…", "環境設定…"]),
    ("Copie de sauvegarde impossible : {}", ["Could not make the backup copy: {}", "No se pudo hacer la copia de seguridad: {}", "Sicherungskopie nicht möglich: {}", "无法创建备份副本：{}", "バックアップを作成できません：{}"]),
    ("Fusionner avec le calque du dessous", ["Merge with the layer below", "Combinar con la capa de abajo", "Mit der Ebene darunter zusammenführen", "与下方图层合并", "下のレイヤーと結合"]),
    ("Fusionner les calques visibles", ["Merge visible layers", "Combinar las capas visibles", "Sichtbare Ebenen zusammenführen", "合并可见图层", "表示中のレイヤーを結合"]),
    ("Il faut au moins deux calques visibles et non verrouillés.", ["At least two visible, unlocked layers are needed.", "Se necesitan al menos dos capas visibles y sin bloquear.", "Mindestens zwei sichtbare, nicht gesperrte Ebenen sind nötig.", "至少需要两个可见且未锁定的图层。", "表示中でロックされていないレイヤーが2つ以上必要です。"]),
    ("Il n'y a pas de calque dessous, ou l'un des deux est verrouillé.", ["There is no layer below, or one of the two is locked.", "No hay capa debajo, o una de las dos está bloqueada.", "Es gibt keine Ebene darunter, oder eine der beiden ist gesperrt.", "下方没有图层，或其中一个已锁定。", "下にレイヤーがないか、どちらかがロックされています。"]),
    ("Espace de travail {}", ["Room {}", "Espacio de trabajo {}", "Arbeitsbereich {}", "工作区 {}", "ルーム {}"]),
    ("Épingler / replier la colonne de gauche", ["Pin / fold the left column", "Fijar / plegar la columna izquierda", "Linke Spalte anheften / einklappen", "固定 / 收起左栏", "左の列を固定 / 折りたたむ"]),
    ("Épingler / replier la colonne de droite", ["Pin / fold the right column", "Fijar / plegar la columna derecha", "Rechte Spalte anheften / einklappen", "固定 / 收起右栏", "右の列を固定 / 折りたたむ"]),
    ("Épingler / replier les deux colonnes", ["Pin / fold both columns", "Fijar / plegar ambas columnas", "Beide Spalten anheften / einklappen", "固定 / 收起两栏", "両方の列を固定 / 折りたたむ"]),
    ("Clip", ["Clip", "Clip", "Clip", "片段", "クリップ"]),
    ("Image§menu", ["Image", "Imagen", "Bild", "图像", "画像"]),
    ("Espaces de travail", ["Rooms", "Espacios de trabajo", "Arbeitsbereiche", "工作区", "ルーム"]),
    ("Nom de l'espace de travail", ["Room name", "Nombre del espacio de trabajo", "Name des Arbeitsbereichs", "工作区名称", "ルーム名"]),
    ("Renommer", ["Rename", "Renombrar", "Umbenennen", "重命名", "名前を変更"]),
    ("Exporter l'espace de travail…", ["Export room…", "Exportar el espacio de trabajo…", "Arbeitsbereich exportieren…", "导出工作区…", "ルームを書き出す…"]),
    ("Espace de travail", ["Room", "Espacio de trabajo", "Arbeitsbereich", "工作区", "ルーム"]),
    ("Importer l'espace de travail…", ["Import room…", "Importar un espacio de trabajo…", "Arbeitsbereich importieren…", "导入工作区…", "ルームを読み込む…"]),
    ("Remettre cet espace par défaut", ["Reset this room", "Restablecer este espacio", "Diesen Arbeitsbereich zurücksetzen", "重置此工作区", "このルームを初期状態に戻す"]),
    ("Colonne de gauche épinglée", ["Left column pinned", "Columna izquierda fijada", "Linke Spalte angeheftet", "左栏固定", "左の列を固定"]),
    ("Colonne de droite épinglée", ["Right column pinned", "Columna derecha fijada", "Rechte Spalte angeheftet", "右栏固定", "右の列を固定"]),
    ("Timeline épinglée", ["Timeline pinned", "Línea de tiempo fijada", "Zeitleiste angeheftet", "时间轴固定", "タイムラインを固定"]),
    ("Corrige les couleurs du dessin courant.", ["Corrects the colours of the current drawing.", "Corrige los colores del dibujo actual.", "Korrigiert die Farben der aktuellen Zeichnung.", "校正当前画的颜色。", "現在の絵の色を補正します。"]),
    ("Glissez pour déplacer le panneau : dans une colonne, ou sur la toile pour le détacher.", ["Drag to move the panel: into a column, or onto the canvas to detach it.", "Arrastre para mover el panel: a una columna, o sobre el lienzo para separarlo.", "Ziehen, um das Feld zu verschieben: in eine Spalte oder auf die Leinwand zum Lösen.", "拖动以移动面板：拖入一栏，或拖到画布上使其浮动。", "ドラッグしてパネルを移動：列の中へ、またはキャンバス上で切り離し。"]),
    ("Fermer le panneau (Fenêtres pour le rouvrir)", ["Close the panel (Windows to open it again)", "Cerrar el panel (Ventanas para reabrirlo)", "Feld schließen (Fenster zum erneuten Öffnen)", "关闭面板（在“窗口”中重新打开）", "パネルを閉じる（ウィンドウで再表示）"]),
    ("Ancrer à droite", ["Dock right", "Anclar a la derecha", "Rechts andocken", "停靠到右侧", "右にドッキング"]),
    ("Ancrer à gauche", ["Dock left", "Anclar a la izquierda", "Links andocken", "停靠到左侧", "左にドッキング"]),
    ("Détacher (fenêtre flottante)", ["Detach (floating window)", "Separar (ventana flotante)", "Lösen (schwebendes Fenster)", "分离（浮动窗口）", "切り離す（フローティング）"]),
    ("📌 Épinglée", ["📌 Pinned", "📌 Fijada", "📌 Angeheftet", "📌 已固定", "📌 固定中"]),
    ("📌 Épingler", ["📌 Pin", "📌 Fijar", "📌 Anheften", "📌 固定", "📌 固定する"]),
    ("Une colonne non épinglée se replie et ressort quand la souris touche le bord.", ["An unpinned column folds away and slides out when the mouse reaches the edge.", "Una columna no fijada se pliega y sale cuando el ratón toca el borde.", "Eine nicht angeheftete Spalte klappt ein und kommt heraus, wenn die Maus den Rand berührt.", "未固定的栏会收起，鼠标碰到边缘时滑出。", "固定しない列は折りたたまれ、マウスが端に触れると出てきます。"]),
    ("Déposez un panneau ici", ["Drop a panel here", "Suelte un panel aquí", "Feld hier ablegen", "将面板拖到这里", "ここにパネルをドロップ"]),
    ("Épingler la timeline (sinon elle se replie et ressort quand la souris la touche)", ["Pin the timeline (otherwise it folds and comes back when the mouse touches it)", "Fijar la línea de tiempo (si no, se pliega y vuelve al tocarla con el ratón)", "Zeitleiste anheften (sonst klappt sie ein und kommt bei Mauskontakt zurück)", "固定时间轴（否则会收起，鼠标碰到时展开）", "タイムラインを固定（オフなら折りたたみ、マウスで展開）"]),
    ("Espace de travail : {}", ["Room: {}", "Espacio de trabajo: {}", "Arbeitsbereich: {}", "工作区：{}", "ルーム：{}"]),
    ("Réglages de l'outil", ["Tool settings", "Ajustes de la herramienta", "Werkzeugeinstellungen", "工具设置", "ツール設定"]),
    ("Correction des couleurs", ["Colour correction", "Corrección de color", "Farbkorrektur", "颜色校正", "色補正"]),
    ("Ce fichier n'est pas un espace de travail.", ["This file is not a room.", "Este archivo no es un espacio de trabajo.", "Diese Datei ist kein Arbeitsbereich.", "此文件不是工作区。", "このファイルはルームではありません。"]),
    ("Espace {}", ["Room {}", "Espacio {}", "Arbeitsbereich {}", "工作区 {}", "ルーム {}"]),
    ("Importer", ["Import", "Importar", "Importieren", "导入", "読み込み"]),
    ("Préférences", ["Preferences", "Preferencias", "Einstellungen", "首选项", "環境設定"]),
    ("Thème", ["Theme", "Tema", "Design", "主题", "テーマ"]),
    ("Calque", ["Layer", "Capa", "Ebene", "图层", "レイヤー"]),
    ("Pochoir", ["Stencil", "Plantilla", "Schablone", "模板", "ステンシル"]),
    ("Allonger le calque tout seul", ["Auto stretch layer", "Alargar la capa automáticamente", "Ebene automatisch verlängern", "自动延长图层", "レイヤーを自動で伸ばす"]),
    ("Peindre après la fin d'un calque l'allonge jusque-là (sinon, la question est posée).", ["Painting after a layer's end makes it longer up to there (otherwise you are asked).", "Pintar después del final de una capa la alarga hasta ahí (si no, se pregunta).", "Malen nach dem Ende einer Ebene verlängert sie bis dorthin (sonst wird gefragt).", "在图层结束后绘画会把图层延长到那里（否则会询问）。", "レイヤーの終わりより後に描くと、そこまで伸ばします（オフなら確認します）。"]),
    ("Fenêtres", ["Windows", "Ventanas", "Fenster", "窗口", "ウィンドウ"]),
    ("Outils, couleurs et réglages (à gauche)", ["Tools, colours and settings (left)", "Herramientas, colores y ajustes (izquierda)", "Werkzeuge, Farben und Einstellungen (links)", "工具、颜色和设置（左侧）", "ツール・色・設定（左）"]),
    ("Préréglages, table lumineuse et son (à droite)", ["Presets, light table and sound (right)", "Preajustes, mesa de luz y sonido (derecha)", "Vorgaben, Leuchttisch und Ton (rechts)", "预设、透写台和声音（右侧）", "プリセット・ライトテーブル・音（右）"]),
    ("Timeline (en bas)", ["Timeline (bottom)", "Línea de tiempo (abajo)", "Zeitleiste (unten)", "时间轴（底部）", "タイムライン（下）"]),
    ("Disposition d'origine", ["Default layout", "Disposición original", "Standardanordnung", "默认布局", "初期レイアウト"]),
    ("Allonger le calque ?", ["Stretch layer?", "¿Alargar la capa?", "Ebene verlängern?", "延长图层？", "レイヤーを伸ばしますか？"]),
    ("Allonger", ["Stretch", "Alargar", "Verlängern", "延长", "伸ばす"]),
    ("Cette image est après la fin du calque « {} ». L'allonger jusqu'à l'image {} ?", ["This frame is after the end of layer “{}”. Stretch it to frame {}?", "Esta imagen está después del final de la capa «{}». ¿Alargarla hasta la imagen {}?", "Dieses Bild liegt nach dem Ende der Ebene „{}“. Bis Bild {} verlängern?", "此帧在图层“{}”结束之后。要延长到第 {} 帧吗？", "このフレームはレイヤー「{}」の終わりより後です。フレーム {} まで伸ばしますか？"]),
    ("Peindre sur une tenue crée un dessin", ["Painting on a hold makes a drawing", "Pintar sobre una exposición crea un dibujo", "Malen auf gehaltenem Bild erzeugt Zeichnung", "在保持帧上绘画会新建画", "保持フレームに描くと新しい絵にする"]),
    ("Dessiner sur une image tenue la transforme en un dessin à elle (copie du dessin tenu).", ["Drawing on a held frame turns it into its own drawing (a copy of the held one).", "Dibujar en una imagen mantenida la convierte en un dibujo propio (copia del mantenido).", "Zeichnen auf ein gehaltenes Bild macht es zu einer eigenen Zeichnung (Kopie der gehaltenen).", "在保持的帧上绘画会把它变成独立的画（复制被保持的画）。", "保持フレームに描くと、そのフレームが独立した絵（保持中の絵のコピー）になります。"]),
    ("Décaler les cellules à gauche", ["Move cells left", "Mover celdas a la izquierda", "Zellen nach links schieben", "单元格左移", "セルを左へ移動"]),
    ("Décaler les cellules à droite", ["Move cells right", "Mover celdas a la derecha", "Zellen nach rechts schieben", "单元格右移", "セルを右へ移動"]),
    ("Cellules", ["Cells", "Celdas", "Zellen", "单元格", "セル"]),
    ("Maj + clic dans la timeline pour en choisir plusieurs.", ["Shift + click in the timeline to pick several.", "Mayús + clic en la línea de tiempo para elegir varias.", "Umschalt + Klick in der Zeitleiste wählt mehrere.", "在时间轴中按 Shift + 点击可多选。", "タイムラインで Shift + クリックすると複数選べます。"]),
    ("Copier les cellules", ["Copy cells", "Copiar celdas", "Zellen kopieren", "复制单元格", "セルをコピー"]),
    ("Coller les cellules", ["Paste cells", "Pegar celdas", "Zellen einfügen", "粘贴单元格", "セルを貼り付け"]),
    ("Tout désélectionner", ["Deselect all", "Deseleccionar todo", "Auswahl aufheben", "全部取消选择", "選択を解除"]),
    ("Repères", ["Marks", "Marcas", "Marken", "标记", "マーク"]),
    ("Signet ici (ajouter / retirer)", ["Toggle bookmark here", "Marcador aquí (poner / quitar)", "Lesezeichen hier (setzen / entfernen)", "在此切换书签", "ここにブックマーク（切替）"]),
    ("Signet précédent", ["Previous bookmark", "Marcador anterior", "Vorheriges Lesezeichen", "上一个书签", "前のブックマーク"]),
    ("Signet suivant", ["Next bookmark", "Marcador siguiente", "Nächstes Lesezeichen", "下一个书签", "次のブックマーク"]),
    ("Retirer tous les signets", ["Remove all bookmarks", "Quitar todos los marcadores", "Alle Lesezeichen entfernen", "移除所有书签", "すべてのブックマークを削除"]),
    ("Début de lecture ici", ["Mark in here", "Inicio de reproducción aquí", "Startmarke hier", "在此设置入点", "ここをインに設定"]),
    ("Fin de lecture ici", ["Mark out here", "Fin de reproducción aquí", "Endmarke hier", "在此设置出点", "ここをアウトに設定"]),
    ("Effacer début et fin de lecture", ["Clear mark in / out", "Borrar inicio y fin", "Start- und Endmarke löschen", "清除入点 / 出点", "イン / アウトを解除"]),
    ("Avant / après", ["Before / after", "Antes / después", "Vorher / nachher", "之前 / 之后", "前 / 後"]),
    ("Avant le premier dessin :", ["Before the first drawing:", "Antes del primer dibujo:", "Vor der ersten Zeichnung:", "第一张画之前：", "最初の絵の前："]),
    ("Après la fin du calque :", ["After the layer's end:", "Después del final de la capa:", "Nach dem Ebenenende:", "图层结束之后：", "レイヤーの終わりの後："]),
    ("Le calque dure jusqu'au bout.", ["The layer lasts to the end.", "La capa dura hasta el final.", "Die Ebene läuft bis zum Ende.", "图层持续到结尾。", "レイヤーは最後まで続きます。"]),
    ("Fin du calque ici", ["Layer ends here", "La capa termina aquí", "Ebene endet hier", "图层在此结束", "レイヤーをここで終える"]),
    ("Jusqu'au bout", ["To the end", "Hasta el final", "Bis zum Ende", "直到结尾", "最後まで"]),
    ("Nom du dessin", ["Drawing name", "Nombre del dibujo", "Name der Zeichnung", "画的名称", "絵の名前"]),
    ("Son en glissant", ["Audio scrub", "Sonido al arrastrar", "Ton beim Ziehen", "拖动时播放声音", "スクラブ音"]),
    ("Jamais", ["Never", "Nunca", "Nie", "从不", "なし"]),
    ("En glissant", ["While dragging", "Al arrastrar", "Beim Ziehen", "拖动时", "ドラッグ中"]),
    ("Toujours", ["Always", "Siempre", "Immer", "总是", "常に"]),
    ("Garder le dessin plus longtemps (du nombre choisi)", ["Hold the drawing longer (by the number chosen)", "Mantener el dibujo más tiempo (el número elegido)", "Zeichnung länger halten (um die gewählte Zahl)", "让画保持更久（按所选数量）", "絵を長く保持（指定の数）"]),
    ("Nombre d'images", ["Number of frames", "Número de imágenes", "Anzahl Bilder", "帧数", "フレーム数"]),
    ("Garder le dessin moins longtemps (du nombre choisi)", ["Hold the drawing shorter (by the number chosen)", "Mantener el dibujo menos tiempo (el número elegido)", "Zeichnung kürzer halten (um die gewählte Zahl)", "让画保持更短（按所选数量）", "絵を短く保持（指定の数）"]),
    ("Régler l'exposition", ["Set exposure", "Fijar exposición", "Belichtung festlegen", "设置曝光", "露出を設定"]),
    ("Chaque dessin des cellules choisies dure exactement ce nombre d'images", ["Each drawing in the chosen cells lasts exactly this many frames", "Cada dibujo de las celdas elegidas dura exactamente este número de imágenes", "Jede Zeichnung der gewählten Zellen dauert genau so viele Bilder", "所选单元格中的每张画恰好持续这么多帧", "選んだセルの各絵をちょうどこのフレーム数にします"]),
    ("Retire les cellules choisies du calque", ["Removes the chosen cells from the layer", "Quita las celdas elegidas de la capa", "Entfernt die gewählten Zellen aus der Ebene", "从图层中移除所选单元格", "選んだセルをレイヤーから削除"]),
    ("{} cellules copiées.", ["{} cells copied.", "{} celdas copiadas.", "{} Zellen kopiert.", "已复制 {} 个单元格。", "{} 個のセルをコピーしました。"]),
    ("Fin du calque : image {}", ["Layer ends at frame {}", "La capa termina en la imagen {}", "Ebene endet bei Bild {}", "图层结束于第 {} 帧", "レイヤーの終わり：フレーム {}"]),
    ("Image {}", ["Frame {}", "Imagen {}", "Bild {}", "第 {} 帧", "フレーム {}"]),
    ("Rien", ["Nothing", "Nada", "Nichts", "无", "なし（空）"]),
    ("Répéter", ["Repeat", "Repetir", "Wiederholen", "重复", "繰り返し"]),
    ("Aller-retour", ["Ping-pong", "Ida y vuelta", "Hin und zurück", "往返", "往復"]),
    ("Tenir", ["Hold", "Mantener", "Halten", "保持", "保持"]),
    ("Tourner la vue de +15°", ["Rotate view +15°", "Girar la vista +15°", "Ansicht um +15° drehen", "视图旋转 +15°", "ビューを +15° 回転"]),
    ("Tourner la vue de −15°", ["Rotate view −15°", "Girar la vista −15°", "Ansicht um −15° drehen", "视图旋转 −15°", "ビューを −15° 回転"]),
    ("Remettre la vue droite", ["Reset view rotation", "Enderezar la vista", "Ansicht gerade stellen", "重置视图旋转", "ビューの回転をリセット"]),
    ("Vue en miroir", ["Mirror view", "Vista en espejo", "Gespiegelte Ansicht", "镜像视图", "ビューを反転"]),
    ("Mode", ["Mode", "Modo", "Modus", "模式", "モード"]),
    ("Préserver la transparence", ["Preserve transparency", "Conservar la transparencia", "Transparenz schützen", "保留透明度", "透明部分を保護"]),
    ("Peindre seulement sur ce qui est déjà dessiné.", ["Paint only over what is already drawn.", "Pintar solo sobre lo ya dibujado.", "Nur auf bereits Gezeichnetes malen.", "只在已绘制的地方上色。", "すでに描かれた部分だけに塗ります。"]),
    ("Lissage du trait", ["Line smoothing", "Suavizado del trazo", "Strichglättung", "线条平滑", "線の手ぶれ補正"]),
    ("Corde", ["Pulled string", "Cuerda", "Schnur", "拉线", "ひも"]),
    ("Moyenne", ["Average", "Promedio", "Mittelwert", "平均", "平均"]),
    ("Points", ["Average points", "Puntos", "Punkte", "平均点", "平均点"]),
    ("Rattraper à la fin", ["Catch up at the end", "Alcanzar al final", "Am Ende aufholen", "结束时追上", "最後に追いつく"]),
    ("Afficher la corde", ["Show the string", "Mostrar la cuerda", "Schnur anzeigen", "显示拉线", "ひもを表示"]),
    ("Peindre dans le pochoir (sinon autour)", ["Paint inside the stencil (otherwise around it)", "Pintar dentro de la plantilla (si no, alrededor)", "In der Schablone malen (sonst drumherum)", "在模板内绘制（否则在周围）", "ステンシルの内側に塗る（オフで外側）"]),
    ("Vue : ", ["View: ", "Vista: ", "Ansicht: ", "视图：", "ビュー："]),
    ("Derrière", ["Behind", "Detrás", "Dahinter", "背后", "背面"]),
    ("Ombrer", ["Shade", "Sombrear", "Abdunkeln", "加深阴影", "陰影"]),
    ("Gommer", ["Erase", "Borrar", "Radieren", "擦除", "消去"]),
    ("Teinter", ["Tint", "Teñir", "Tönen", "着色", "色付け"]),
    ("Coloriser", ["Colorize", "Colorear", "Kolorieren", "上色", "カラー化"]),
    ("Saturer", ["Saturate", "Saturar", "Sättigen", "增加饱和度", "彩度を上げる"]),
    ("Désaturer", ["Desaturate", "Desaturar", "Entsättigen", "去饱和", "彩度を下げる"]),
    ("Densité −", ["Dodge", "Sobreexponer", "Abwedeln", "减淡", "覆い焼き"]),
    ("Densité +", ["Burn", "Subexponer", "Nachbelichten", "加深", "焼き込み"]),
    ("Éclaircir (max)", ["Lighten", "Aclarar", "Aufhellen", "变亮", "比較（明）"]),
    ("Position", ["Position", "Posición", "Position", "位置", "位置"]),
    ("Table lumineuse", ["Light table", "Mesa de luz", "Leuchttisch", "透写台", "ライトテーブル"]),
    ("Langue", ["Language", "Idioma", "Sprache", "语言", "言語"]),
    (" · modifié", [" · modified", " · modificado", " · geändert", " · 已修改", " · 変更あり"]),
    ("i/s", ["fps", "i/s", "B/s", "帧/秒", "fps"]),
    // tools panel
    ("Outils", ["Tools", "Herramientas", "Werkzeuge", "工具", "ツール"]),
    ("Raccourci : {}", ["Shortcut: {}", "Atajo: {}", "Tastenkürzel: {}", "快捷键：{}", "ショートカット：{}"]),
    ("Réglages : {}", ["Settings: {}", "Ajustes: {}", "Einstellungen: {}", "设置：{}", "設定：{}"]),
    ("Taille", ["Size", "Tamaño", "Größe", "大小", "サイズ"]),
    ("Tolérance", ["Tolerance", "Tolerancia", "Toleranz", "容差", "許容値"]),
    ("Puissance", ["Power", "Potencia", "Stärke", "强度", "強さ"]),
    ("Opacité", ["Opacity", "Opacidad", "Deckkraft", "不透明度", "不透明度"]),
    ("Aspect", ["Aspect", "Aspecto", "Seitenverhältnis", "纵横比", "縦横比"]),
    ("Angle", ["Angle", "Ángulo", "Winkel", "角度", "角度"]),
    ("Espacement : {} px · force par touche : {} %", ["Spacing: {} px · strength per dab: {} %", "Espaciado: {} px · fuerza por toque: {} %", "Abstand: {} px · Stärke pro Tupfer: {} %", "间距：{} px · 每次笔触强度：{} %", "間隔：{} px · 1打点の強さ：{} %"]),
    ("Valeurs par défaut", ["Defaults", "Valores por defecto", "Standardwerte", "默认值", "初期値"]),
    ("Couleurs", ["Colours", "Colores", "Farben", "颜色", "色"]),
    ("Échanger les couleurs (X)", ["Swap colours (X)", "Intercambiar colores (X)", "Farben tauschen (X)", "交换颜色 (X)", "色を入れ替え (X)"]),
    ("Couleur B", ["Colour B", "Color B", "Farbe B", "颜色 B", "色 B"]),
    ("Clic : couleur A · clic droit : couleur B", ["Click: colour A · right-click: colour B", "Clic: color A · clic derecho: color B", "Klick: Farbe A · Rechtsklick: Farbe B", "单击：颜色 A · 右键：颜色 B", "クリック：色A · 右クリック：色B"]),
    // tools
    ("Crayon", ["Pencil", "Lápiz", "Bleistift", "铅笔", "鉛筆"]),
    ("Stylo", ["Pen", "Pluma", "Stift", "钢笔", "ペン"]),
    ("Aérographe", ["Airbrush", "Aerógrafo", "Airbrush", "喷枪", "エアブラシ"]),
    ("Pinceau", ["Brush", "Pincel", "Pinsel", "画笔", "筆"]),
    ("Pinceau humide", ["Wet brush", "Pincel húmedo", "Nasser Pinsel", "湿画笔", "水筆"]),
    ("Gomme", ["Eraser", "Goma", "Radierer", "橡皮擦", "消しゴム"]),
    ("Remplissage", ["Fill", "Relleno", "Füllen", "填充", "塗りつぶし"]),
    ("Ligne", ["Line", "Línea", "Linie", "直线", "直線"]),
    ("Rectangle", ["Rectangle", "Rectángulo", "Rechteck", "矩形", "長方形"]),
    ("Ellipse", ["Ellipse", "Elipse", "Ellipse", "椭圆", "楕円"]),
    ("Pipette", ["Picker", "Cuentagotas", "Pipette", "吸管", "スポイト"]),
    // light table panel
    ("Activée (T)", ["On (T)", "Activada (T)", "Aktiv (T)", "开启 (T)", "オン (T)"]),
    ("Avant", ["Before", "Antes", "Vorher", "之前", "前"]),
    ("Après", ["After", "Después", "Nachher", "之后", "後"]),
    ("Teinte avant", ["Tint before", "Tinte antes", "Farbe vorher", "之前着色", "前の色"]),
    ("Teinte après", ["Tint after", "Tinte después", "Farbe nachher", "之后着色", "後の色"]),
    ("Devant le dessin", ["In front of drawing", "Delante del dibujo", "Vor der Zeichnung", "显示在画面前方", "絵の前に表示"]),
    ("Opacité par image :", ["Opacity per frame:", "Opacidad por imagen:", "Deckkraft pro Bild:", "每帧不透明度：", "画像ごとの不透明度："]),
    ("Projet", ["Project", "Proyecto", "Projekt", "项目", "プロジェクト"]),
    ("Taille : {} × {}", ["Size: {} × {}", "Tamaño: {} × {}", "Größe: {} × {}", "尺寸：{} × {}", "サイズ：{} × {}"]),
    ("Papier", ["Paper", "Papel", "Papier", "纸张", "紙"]),
    ("Calques : {}", ["Layers: {}", "Capas: {}", "Ebenen: {}", "图层：{}", "レイヤー：{}"]),
    ("Durée : {} s", ["Duration: {} s", "Duración: {} s", "Dauer: {} s", "时长：{} 秒", "長さ：{} 秒"]),
    ("Raccourcis", ["Shortcuts", "Atajos", "Tastenkürzel", "快捷键", "ショートカット"]),
    ("Entrée", ["Enter", "Intro", "Eingabe", "回车", "Enter"]),
    ("lire / arrêter", ["play / stop", "reproducir / detener", "abspielen / stoppen", "播放 / 停止", "再生 / 停止"]),
    ("image précédente / suivante", ["previous / next frame", "imagen anterior / siguiente", "vorheriges / nächstes Bild", "上一帧 / 下一帧", "前 / 次の画像"]),
    ("calque au-dessus / dessous", ["layer above / below", "capa superior / inferior", "Ebene darüber / darunter", "上方 / 下方图层", "上 / 下のレイヤー"]),
    ("taille du pinceau", ["brush size", "tamaño del pincel", "Pinselgröße", "画笔大小", "ブラシサイズ"]),
    ("Espace + glisser", ["Space + drag", "Espacio + arrastrar", "Leertaste + ziehen", "空格 + 拖动", "Space + ドラッグ"]),
    ("déplacer la vue", ["move the view", "mover la vista", "Ansicht verschieben", "移动视图", "表示を移動"]),
    ("Molette", ["Wheel", "Rueda", "Mausrad", "滚轮", "ホイール"]),
    ("zoom", ["zoom", "zoom", "Zoom", "缩放", "ズーム"]),
    ("annuler / rétablir", ["undo / redo", "deshacer / rehacer", "rückgängig / wiederholen", "撤销 / 重做", "元に戻す / やり直す"]),
    ("Suppr", ["Del", "Supr", "Entf", "Del", "Del"]),
    ("effacer le dessin", ["clear drawing", "borrar el dibujo", "Zeichnung löschen", "清除画面", "絵を消去"]),
    // colour groups
    ("Groupes de couleur", ["Colour groups", "Grupos de color", "Farbgruppen", "颜色组", "カラーグループ"]),
    ("Seulement ce groupe dans la timeline", ["Only this group in the timeline", "Solo este grupo en la línea de tiempo", "Nur diese Gruppe in der Zeitleiste", "时间轴中仅显示此组", "タイムラインにこのグループのみ"]),
    ("Tous les groupes dans la timeline", ["Every group in the timeline", "Todos los grupos en la línea de tiempo", "Alle Gruppen in der Zeitleiste", "时间轴中显示所有组", "タイムラインにすべてのグループ"]),
    ("Ce groupe dans la timeline", ["This group in the timeline", "Este grupo en la línea de tiempo", "Diese Gruppe in der Zeitleiste", "时间轴中显示此组", "タイムラインにこのグループ"]),
    ("Tout afficher", ["List all", "Mostrar todos", "Alle zeigen", "全部显示", "すべて表示"]),
    ("Tout masquer", ["List none", "Ocultar todos", "Alle ausblenden", "全部隐藏", "すべて隠す"]),
    ("Groupes affichés dans ce panneau", ["Groups listed in this panel", "Grupos mostrados en este panel", "In diesem Feld gezeigte Gruppen", "此面板中显示的组", "このパネルに表示するグループ"]),
    ("Protéger le groupe (sa couleur et ses calques verrouillés)", ["Protect the group (its colour and its locked layers)", "Proteger el grupo (su color y sus capas bloqueadas)", "Gruppe schützen (Farbe und gesperrte Ebenen)", "保护该组（颜色和锁定的图层）", "グループを保護（色とロックされたレイヤー）"]),
    ("Verrouiller les calques du groupe", ["Lock the group's layers", "Bloquear las capas del grupo", "Ebenen der Gruppe sperren", "锁定该组图层", "グループのレイヤーをロック"]),
    ("Déverrouiller les calques du groupe", ["Unlock the group's layers", "Desbloquear las capas del grupo", "Ebenen der Gruppe entsperren", "解锁该组图层", "グループのレイヤーのロックを解除"]),
    ("Masquer les autres groupes", ["Hide the other groups", "Ocultar los otros grupos", "Andere Gruppen ausblenden", "隐藏其他组", "他のグループを隠す"]),
    ("Afficher les autres groupes", ["Show the other groups", "Mostrar los otros grupos", "Andere Gruppen zeigen", "显示其他组", "他のグループを表示"]),
    ("Mettre le calque courant dans ce groupe", ["Put the current layer in this group", "Poner la capa actual en este grupo", "Aktuelle Ebene in diese Gruppe", "将当前图层放入此组", "現在のレイヤーをこのグループに"]),
    ("Protéger / déprotéger les calques du groupe", ["Protect / unprotect the group's layers", "Proteger / desproteger las capas del grupo", "Ebenen der Gruppe schützen / freigeben", "保护 / 取消保护该组图层", "グループのレイヤーを保護 / 解除"]),
    ("{} calques", ["{} layers", "{} capas", "{} Ebenen", "{} 个图层", "{} レイヤー"]),
    ("Sans couleur", ["No colour", "Sin color", "Keine Farbe", "无颜色", "色なし"]),
    ("Rouge", ["Red", "Rojo", "Rot", "红", "赤"]),
    ("Orange", ["Orange", "Naranja", "Orange", "橙", "オレンジ"]),
    ("Jaune", ["Yellow", "Amarillo", "Gelb", "黄", "黄"]),
    ("Vert clair", ["Lime", "Lima", "Hellgrün", "浅绿", "黄緑"]),
    ("Vert", ["Green", "Verde", "Grün", "绿", "緑"]),
    ("Bleu-vert", ["Teal", "Verde azulado", "Blaugrün", "青绿", "青緑"]),
    ("Bleu", ["Blue", "Azul", "Blau", "蓝", "青"]),
    ("Indigo", ["Indigo", "Índigo", "Indigo", "靛蓝", "藍"]),
    ("Violet", ["Violet", "Violeta", "Violett", "紫", "紫"]),
    ("Rose", ["Pink", "Rosa", "Rosa", "粉", "ピンク"]),
    ("Prune", ["Plum", "Ciruela", "Pflaume", "梅紫", "プラム"]),
    ("Blanc", ["White", "Blanco", "Weiß", "白", "白"]),
    // remote panel
    ("Télécommande", ["Remote", "Mando", "Fernbedienung", "遥控器", "リモコン"]),
    ("Son activé / coupé", ["Sound on / off", "Sonido activado / desactivado", "Ton an / aus", "声音开 / 关", "サウンド オン / オフ"]),
    ("Arrêter et revenir au début", ["Stop and go back to the start", "Detener y volver al inicio", "Stoppen und zum Anfang", "停止并回到开头", "停止して最初に戻る"]),
    ("Dessin précédent", ["Previous drawing", "Dibujo anterior", "Vorherige Zeichnung", "上一张画", "前の絵"]),
    ("Dessin suivant", ["Next drawing", "Dibujo siguiente", "Nächste Zeichnung", "下一张画", "次の絵"]),
    ("Image {} / {}", ["Frame {} / {}", "Imagen {} / {}", "Bild {} / {}", "第 {} / {} 帧", "画像 {} / {}"]),
    // timeline
    ("Première image", ["First frame", "Primera imagen", "Erstes Bild", "第一帧", "最初の画像"]),
    ("Image précédente", ["Previous frame", "Imagen anterior", "Vorheriges Bild", "上一帧", "前の画像"]),
    ("⏸ Arrêter", ["⏸ Stop", "⏸ Detener", "⏸ Stopp", "⏸ 停止", "⏸ 停止"]),
    ("▶ Lire", ["▶ Play", "▶ Reproducir", "▶ Abspielen", "▶ 播放", "▶ 再生"]),
    ("Image suivante", ["Next frame", "Imagen siguiente", "Nächstes Bild", "下一帧", "次の画像"]),
    ("Dernière image", ["Last frame", "Última imagen", "Letztes Bild", "最后一帧", "最後の画像"]),
    ("Boucle", ["Loop", "Bucle", "Schleife", "循环", "ループ"]),
    ("Image", ["Frame", "Imagen", "Bild", "帧", "画像"]),
    ("Cadence", ["Frame rate", "Velocidad", "Bildrate", "帧率", "フレームレート"]),
    ("Durée", ["Length", "Duración", "Dauer", "时长", "長さ"]),
    ("Nombre d'images du projet", ["Number of frames in the project", "Número de imágenes del proyecto", "Anzahl der Bilder im Projekt", "项目帧数", "プロジェクトの画像数"]),
    ("+ Image", ["+ Frame", "+ Imagen", "+ Bild", "+ 帧", "+ 画像"]),
    ("Nouvelle image vierge après l'image courante", ["New blank frame after the current one", "Nueva imagen en blanco después de la actual", "Neues leeres Bild nach dem aktuellen", "在当前帧后新建空白帧", "現在の画像の後に白紙の画像"]),
    ("Dupliquer", ["Duplicate", "Duplicar", "Duplizieren", "复制", "複製"]),
    ("Copie du dessin courant après lui", ["Copy of the current drawing after it", "Copia del dibujo actual después de él", "Kopie der aktuellen Zeichnung dahinter", "在其后复制当前画面", "現在の絵の後にコピー"]),
    ("Exposition +", ["Hold +", "Exposición +", "Belichtung +", "曝光 +", "露出 +"]),
    ("Garder le dessin une image de plus", ["Hold the drawing one more frame", "Mantener el dibujo una imagen más", "Zeichnung ein Bild länger halten", "画面多停留一帧", "絵を1コマ長く"]),
    ("Exposition −", ["Hold −", "Exposición −", "Belichtung −", "曝光 −", "露出 −"]),
    ("Garder le dessin une image de moins", ["Hold the drawing one frame less", "Mantener el dibujo una imagen menos", "Zeichnung ein Bild kürzer halten", "画面少停留一帧", "絵を1コマ短く"]),
    ("Supprimer la cellule", ["Delete cell", "Eliminar la celda", "Zelle löschen", "删除单元格", "セルを削除"]),
    ("Retire la cellule courante du calque", ["Removes the current cell from the layer", "Quita la celda actual de la capa", "Entfernt die aktuelle Zelle aus der Ebene", "从图层中移除当前单元格", "レイヤーから現在のセルを削除"]),
    ("+ Calque", ["+ Layer", "+ Capa", "+ Ebene", "+ 图层", "+ レイヤー"]),
    ("Nouveau calque au-dessus", ["New layer above", "Nueva capa encima", "Neue Ebene darüber", "在上方新建图层", "上に新規レイヤー"]),
    ("Dupliquer le calque", ["Duplicate layer", "Duplicar capa", "Ebene duplizieren", "复制图层", "レイヤーを複製"]),
    ("Copie du calque courant", ["Copy of the current layer", "Copia de la capa actual", "Kopie der aktuellen Ebene", "复制当前图层", "現在のレイヤーのコピー"]),
    ("Supprimer le calque", ["Delete layer", "Eliminar capa", "Ebene löschen", "删除图层", "レイヤーを削除"]),
    ("Retire le calque courant", ["Removes the current layer", "Quita la capa actual", "Entfernt die aktuelle Ebene", "移除当前图层", "現在のレイヤーを削除"]),
    ("Monter", ["Up", "Subir", "Nach oben", "上移", "上へ"]),
    ("Monter le calque", ["Move layer up", "Subir la capa", "Ebene nach oben", "上移图层", "レイヤーを上へ"]),
    ("Descendre", ["Down", "Bajar", "Nach unten", "下移", "下へ"]),
    ("Descendre le calque", ["Move layer down", "Bajar la capa", "Ebene nach unten", "下移图层", "レイヤーを下へ"]),
    ("Mode de mélange du calque", ["Layer blending mode", "Modo de fusión de la capa", "Mischmodus der Ebene", "图层混合模式", "レイヤーの合成モード"]),
    ("Couleur", ["Colour", "Color", "Farbe", "颜色", "色"]),
    ("Groupe de couleur du calque", ["Layer colour group", "Grupo de color de la capa", "Farbgruppe der Ebene", "图层颜色组", "レイヤーのカラーグループ"]),
    ("Calques", ["Layers", "Capas", "Ebenen", "图层", "レイヤー"]),
    // blending modes
    ("Normal", ["Normal", "Normal", "Normal", "正常", "通常"]),
    ("Produit", ["Multiply", "Multiplicar", "Multiplizieren", "正片叠底", "乗算"]),
    ("Superposition claire", ["Screen", "Trama", "Negativ multiplizieren", "滤色", "スクリーン"]),
    ("Addition", ["Add", "Sumar", "Addieren", "相加", "加算"]),
    ("Incrustation", ["Overlay", "Superponer", "Ineinanderkopieren", "叠加", "オーバーレイ"]),
    ("Assombrir", ["Darken", "Oscurecer", "Abdunkeln", "变暗", "比較（暗）"]),
    ("Éclaircir", ["Lighten", "Aclarar", "Aufhellen", "变亮", "比較（明）"]),
    ("Différence", ["Difference", "Diferencia", "Differenz", "差值", "差の絶対値"]),
    ("Gommer dessous", ["Erase below", "Borrar debajo", "Darunter radieren", "擦除下方", "下を消去"]),
    // papers
    ("Lisse", ["Smooth", "Liso", "Glatt", "光滑", "なめらか"]),
    ("Grain fin", ["Fine grain", "Grano fino", "Feine Körnung", "细纹", "細目"]),
    ("Toile", ["Canvas", "Lienzo", "Leinwand", "画布", "キャンバス"]),
    ("Aquarelle", ["Watercolour", "Acuarela", "Aquarell", "水彩", "水彩"]),
    // new project window
    ("Nouveau projet", ["New project", "Nuevo proyecto", "Neues Projekt", "新建项目", "新規プロジェクト"]),
    ("Format", ["Format", "Formato", "Format", "格式", "フォーマット"]),
    ("Largeur", ["Width", "Ancho", "Breite", "宽度", "幅"]),
    ("Hauteur", ["Height", "Alto", "Höhe", "高度", "高さ"]),
    ("Le projet en cours sera fermé (pensez à l'enregistrer).", ["The current project will be closed (remember to save it).", "El proyecto actual se cerrará (recuerda guardarlo).", "Das aktuelle Projekt wird geschlossen (vorher speichern).", "当前项目将被关闭（请先保存）。", "現在のプロジェクトは閉じられます（保存をお忘れなく）。"]),
    ("Créer", ["Create", "Crear", "Erstellen", "创建", "作成"]),
    ("Fermer", ["Cancel", "Cancelar", "Abbrechen", "取消", "キャンセル"]),
    // text tool
    ("Texte", ["Text", "Texto", "Text", "文字", "テキスト"]),
    ("Bordure", ["Border", "Borde", "Rand", "描边", "縁取り"]),
    ("Italique", ["Italic", "Cursiva", "Kursiv", "斜体", "斜体"]),
    ("Rotation", ["Rotation", "Rotación", "Drehung", "旋转", "回転"]),
    ("Espacement des lettres", ["Letter spacing", "Espaciado de letras", "Buchstabenabstand", "字间距", "文字間隔"]),
    ("Cliquez sur la toile pour poser le texte (coin haut-gauche).", ["Click on the canvas to place the text (top-left corner).", "Haz clic en el lienzo para colocar el texto (esquina superior izquierda).", "Auf die Leinwand klicken, um den Text zu setzen (obere linke Ecke).", "在画布上单击以放置文字（左上角）。", "キャンバスをクリックしてテキストを配置（左上の角）。"]),
    // pressure
    ("Pression → taille", ["Pressure → size", "Presión → tamaño", "Druck → Größe", "压力 → 大小", "筆圧 → サイズ"]),
    ("Pression → puissance", ["Pressure → power", "Presión → potencia", "Druck → Stärke", "压力 → 强度", "筆圧 → 強さ"]),
    ("Simuler la pression avec la vitesse (souris)", ["Simulate pressure from speed (mouse)", "Simular la presión con la velocidad (ratón)", "Druck aus der Geschwindigkeit simulieren (Maus)", "用速度模拟压力（鼠标）", "速度で筆圧をシミュレート（マウス）"]),
    ("Sans tablette : plus le trait est rapide, plus il est fin.", ["Without a tablet: the faster the stroke, the thinner it is.", "Sin tableta: cuanto más rápido el trazo, más fino.", "Ohne Tablett: je schneller der Strich, desto dünner.", "没有数位板时：笔画越快越细。", "タブレットなし：速く描くほど線が細くなります。"]),
    // sound
    ("Son", ["Sound", "Sonido", "Ton", "声音", "サウンド"]),
    ("Importer un son (WAV, MP3)…", ["Import a sound (WAV, MP3)…", "Importar un sonido (WAV, MP3)…", "Ton importieren (WAV, MP3)…", "导入声音（WAV、MP3）…", "サウンドを読み込む（WAV、MP3）…"]),
    ("Fichier introuvable", ["File not found", "Archivo no encontrado", "Datei nicht gefunden", "找不到文件", "ファイルが見つかりません"]),
    ("Muet", ["Mute", "Silencio", "Stumm", "静音", "ミュート"]),
    ("Volume", ["Volume", "Volumen", "Lautstärke", "音量", "音量"]),
    ("Décalage", ["Offset", "Desfase", "Versatz", "偏移", "オフセット"]),
    ("images", ["frames", "imágenes", "Bilder", "帧", "コマ"]),
    ("Fondu", ["Fade", "Fundido", "Blende", "淡入淡出", "フェード"]),
    ("Fondu d'entrée", ["Fade in", "Fundido de entrada", "Einblenden", "淡入", "フェードイン"]),
    ("Fondu de sortie", ["Fade out", "Fundido de salida", "Ausblenden", "淡出", "フェードアウト"]),
    ("Remplacer…", ["Replace…", "Reemplazar…", "Ersetzen…", "替换…", "置き換え…"]),
    ("Retirer le son", ["Remove sound", "Quitar el sonido", "Ton entfernen", "移除声音", "サウンドを外す"]),
    ("Son introuvable ou illisible : {}", ["Sound missing or unreadable: {}", "Sonido no encontrado o ilegible: {}", "Ton fehlt oder ist unlesbar: {}", "声音缺失或无法读取：{}", "サウンドが見つからないか読めません：{}"]),
    ("Son importé : {} ({} s)", ["Sound imported: {} ({} s)", "Sonido importado: {} ({} s)", "Ton importiert: {} ({} s)", "已导入声音：{}（{} 秒）", "サウンドを読み込みました：{}（{} 秒）"]),
    ("Lecture du son impossible : {}", ["Cannot play the sound: {}", "No se puede reproducir el sonido: {}", "Ton kann nicht abgespielt werden: {}", "无法播放声音：{}", "サウンドを再生できません：{}"]),
    // selection, transform, gradient
    ("Sélection", ["Selection", "Selección", "Auswahl", "选区", "選択"]),
    ("Dégradé", ["Gradient", "Degradado", "Verlauf", "渐变", "グラデーション"]),
    ("Lasso", ["Lasso", "Lazo", "Lasso", "套索", "なげなわ"]),
    ("Baguette", ["Magic wand", "Varita", "Zauberstab", "魔棒", "自動選択"]),
    ("Maj : ajouter · Alt : retirer", ["Shift: add · Alt: subtract", "Mayús: añadir · Alt: quitar", "Umschalt: hinzufügen · Alt: abziehen", "Shift：添加 · Alt：减去", "Shift：追加 · Alt：削除"]),
    ("Tout", ["All", "Todo", "Alles", "全部", "すべて"]),
    ("Inverser", ["Invert", "Invertir", "Umkehren", "反选", "反転"]),
    ("Aucune", ["None", "Ninguna", "Keine", "取消", "解除"]),
    ("Copier", ["Copy", "Copiar", "Kopieren", "复制", "コピー"]),
    ("Couper", ["Cut", "Cortar", "Ausschneiden", "剪切", "切り取り"]),
    ("Coller", ["Paste", "Pegar", "Einfügen", "粘贴", "貼り付け"]),
    ("Effacer", ["Erase", "Borrar", "Löschen", "清除", "消去"]),
    ("Transformer", ["Transform", "Transformar", "Transformieren", "变换", "変形"]),
    ("Miroir ↔", ["Mirror ↔", "Espejo ↔", "Spiegeln ↔", "水平翻转", "左右反転"]),
    ("Miroir ↕", ["Mirror ↕", "Espejo ↕", "Spiegeln ↕", "垂直翻转", "上下反転"]),
    ("Transformation en cours", ["Transforming", "Transformando", "Transformation läuft", "正在变换", "変形中"]),
    ("Échelle", ["Scale", "Escala", "Skalierung", "缩放", "拡大縮小"]),
    ("Glissez sur la toile pour déplacer.", ["Drag on the canvas to move.", "Arrastra en el lienzo para mover.", "Auf der Leinwand ziehen zum Verschieben.", "在画布上拖动以移动。", "キャンバスをドラッグして移動。"]),
    ("Valider (Entrée)", ["Apply (Enter)", "Aplicar (Intro)", "Anwenden (Eingabe)", "应用（回车）", "確定（Enter）"]),
    ("Annuler (Échap)", ["Cancel (Esc)", "Cancelar (Esc)", "Abbrechen (Esc)", "取消（Esc）", "キャンセル（Esc）"]),
    ("Linéaire", ["Linear", "Lineal", "Linear", "线性", "線形"]),
    ("Radial", ["Radial", "Radial", "Radial", "径向", "円形"]),
    ("Milieu", ["Midpoint", "Punto medio", "Mitte", "中点", "中間点"]),
    ("Glissez sur la toile : de la couleur A à la couleur B, dans la sélection s'il y en a une.", ["Drag on the canvas: from colour A to colour B, inside the selection if there is one.", "Arrastra en el lienzo: del color A al color B, dentro de la selección si la hay.", "Auf der Leinwand ziehen: von Farbe A zu Farbe B, in der Auswahl, falls vorhanden.", "在画布上拖动：从颜色 A 到颜色 B，若有选区则只在选区内。", "キャンバスをドラッグ：色Aから色Bへ、選択範囲があればその中に。"]),
    // shortcuts and settings
    ("Réglages", ["Settings", "Ajustes", "Einstellungen", "设置", "設定"]),
    ("Exporter mes réglages (raccourcis, préréglages, langue, thème, palette)…", ["Export my settings (shortcuts, presets, language, theme, palette)…", "Exportar mis ajustes (atajos, ajustes guardados, idioma, tema, paleta)…", "Meine Einstellungen exportieren (Tastenkürzel, Voreinstellungen, Sprache, Design, Palette)…", "导出我的设置（快捷键、预设、语言、主题、调色板）…", "設定を書き出す（ショートカット、プリセット、言語、テーマ、パレット）…"]),
    ("Importer des réglages…", ["Import settings…", "Importar ajustes…", "Einstellungen importieren…", "导入设置…", "設定を読み込む…"]),
    ("Raccourcis clavier…", ["Keyboard shortcuts…", "Atajos de teclado…", "Tastenkürzel…", "键盘快捷键…", "キーボードショートカット…"]),
    ("Raccourcis clavier", ["Keyboard shortcuts", "Atajos de teclado", "Tastenkürzel", "键盘快捷键", "キーボードショートカット"]),
    ("Chercher…", ["Search…", "Buscar…", "Suchen…", "搜索…", "検索…"]),
    ("Tout par défaut", ["All to defaults", "Todo por defecto", "Alles auf Standard", "全部恢复默认", "すべて初期値に"]),
    ("Exporter (.cfg)…", ["Export (.cfg)…", "Exportar (.cfg)…", "Exportieren (.cfg)…", "导出 (.cfg)…", "書き出し (.cfg)…"]),
    ("Importer (.cfg)…", ["Import (.cfg)…", "Importar (.cfg)…", "Importieren (.cfg)…", "导入 (.cfg)…", "読み込み (.cfg)…"]),
    ("Changer", ["Change", "Cambiar", "Ändern", "更改", "変更"]),
    ("Ajouter un autre raccourci", ["Add another shortcut", "Añadir otro atajo", "Weiteres Kürzel hinzufügen", "添加另一个快捷键", "別のショートカットを追加"]),
    ("Aucun", ["None", "Ninguno", "Keins", "无", "なし"]),
    ("Par défaut", ["Default", "Por defecto", "Standard", "默认", "初期値"]),
    ("Copier, couper et coller suivent les raccourcis du système. Les raccourcis sont enregistrés tout de suite.", ["Copy, cut and paste follow the system's shortcuts. Shortcuts are saved right away.", "Copiar, cortar y pegar siguen los atajos del sistema. Los atajos se guardan al instante.", "Kopieren, Ausschneiden und Einfügen folgen den Systemkürzeln. Kürzel werden sofort gespeichert.", "复制、剪切和粘贴使用系统快捷键。快捷键会立即保存。", "コピー・切り取り・貼り付けはシステムのショートカットに従います。ショートカットはすぐに保存されます。"]),
    ("{} était utilisé par « {} » : il ne l'est plus.", ["{} was used by “{}”: it isn't any more.", "{} lo usaba «{}»: ya no.", "{} war „{}“ zugewiesen: jetzt nicht mehr.", "{} 原本用于“{}”：已取消。", "{} は「{}」に使われていました：解除しました。"]),
    ("Appuyez sur la nouvelle touche pour « {} » (Échap : annuler)", ["Press the new key for “{}” (Esc: cancel)", "Pulsa la nueva tecla para «{}» (Esc: cancelar)", "Neue Taste für „{}“ drücken (Esc: abbrechen)", "为“{}”按下新按键（Esc：取消）", "「{}」の新しいキーを押してください（Esc：キャンセル）"]),
    ("Raccourcis importés ({} lignes ignorées).", ["Shortcuts imported ({} lines skipped).", "Atajos importados ({} líneas ignoradas).", "Tastenkürzel importiert ({} Zeilen übersprungen).", "已导入快捷键（忽略 {} 行）。", "ショートカットを読み込みました（{} 行をスキップ）。"]),
    ("Réglages exportés : {}", ["Settings exported: {}", "Ajustes exportados: {}", "Einstellungen exportiert: {}", "已导出设置：{}", "設定を書き出しました：{}"]),
    ("Réglages importés : {}", ["Settings imported: {}", "Ajustes importados: {}", "Einstellungen importiert: {}", "已导入设置：{}", "設定を読み込みました：{}"]),
    ("Ce fichier ne contient pas de réglages.", ["This file doesn't contain settings.", "Este archivo no contiene ajustes.", "Diese Datei enthält keine Einstellungen.", "此文件不包含设置。", "このファイルには設定が含まれていません。"]),
    ("Changer…", ["Change…", "Cambiar…", "Ändern…", "更改…", "変更…"]),
    // action names (shortcuts)
    ("Lire / arrêter", ["Play / stop", "Reproducir / detener", "Abspielen / stoppen", "播放 / 停止", "再生 / 停止"]),
    ("Calque au-dessus", ["Layer above", "Capa superior", "Ebene darüber", "上方图层", "上のレイヤー"]),
    ("Calque en dessous", ["Layer below", "Capa inferior", "Ebene darunter", "下方图层", "下のレイヤー"]),
    ("Effacer la sélection ou le dessin", ["Erase the selection or the drawing", "Borrar la selección o el dibujo", "Auswahl oder Zeichnung löschen", "清除选区或画面", "選択範囲または絵を消去"]),
    ("Échanger les couleurs", ["Swap colours", "Intercambiar colores", "Farben tauschen", "交换颜色", "色を入れ替え"]),
    ("Pinceau plus petit", ["Smaller brush", "Pincel más pequeño", "Kleinerer Pinsel", "缩小画笔", "ブラシを小さく"]),
    ("Pinceau plus grand", ["Bigger brush", "Pincel más grande", "Größerer Pinsel", "放大画笔", "ブラシを大きく"]),
    ("Feuilleter : dessin précédent (maintenir)", ["Flip: previous drawing (hold)", "Hojear: dibujo anterior (mantener)", "Blättern: vorherige Zeichnung (halten)", "翻页：上一张（按住）", "パラパラ：前の絵（長押し）"]),
    ("Feuilleter : dessin suivant (maintenir)", ["Flip: next drawing (hold)", "Hojear: dibujo siguiente (mantener)", "Blättern: nächste Zeichnung (halten)", "翻页：下一张（按住）", "パラパラ：次の絵（長押し）"]),
    ("Tout sélectionner", ["Select all", "Seleccionar todo", "Alles auswählen", "全选", "すべて選択"]),
    ("Désélectionner", ["Deselect", "Deseleccionar", "Auswahl aufheben", "取消选择", "選択解除"]),
    ("Outil : {}", ["Tool: {}", "Herramienta: {}", "Werkzeug: {}", "工具：{}", "ツール：{}"]),
    // clips and storyboard
    ("Plan", ["Shot", "Plano", "Einstellung", "镜头", "カット"]),
    ("Storyboard", ["Storyboard", "Storyboard", "Storyboard", "故事板", "絵コンテ"]),
    ("Storyboard (plans)…", ["Storyboard (shots)…", "Storyboard (planos)…", "Storyboard (Einstellungen)…", "故事板（镜头）…", "絵コンテ（カット）…"]),
    ("Film complet (tous les plans) en PNG…", ["Whole film (every shot) as PNG…", "Película completa (todos los planos) en PNG…", "Ganzer Film (alle Einstellungen) als PNG…", "完整影片（所有镜头）PNG…", "フィルム全体（全カット）PNG…"]),
    ("Film complet (tous les plans) en MP4…", ["Whole film (every shot) as MP4…", "Película completa (todos los planos) en MP4…", "Ganzer Film (alle Einstellungen) als MP4…", "完整影片（所有镜头）MP4…", "フィルム全体（全カット）MP4…"]),
    ("+ Nouveau plan", ["+ New shot", "+ Nuevo plano", "+ Neue Einstellung", "+ 新镜头", "+ 新しいカット"]),
    ("Dupliquer le plan", ["Duplicate shot", "Duplicar el plano", "Einstellung duplizieren", "复制镜头", "カットを複製"]),
    ("Lire tous les plans à la suite", ["Play every shot in a row", "Reproducir todos los planos seguidos", "Alle Einstellungen nacheinander abspielen", "连续播放所有镜头", "全カットを続けて再生"]),
    ("Notes (action, dialogue…)", ["Notes (action, dialogue…)", "Notas (acción, diálogo…)", "Notizen (Handlung, Dialog…)", "备注（动作、对白…）", "メモ（演技、セリフ…）"]),
    ("Ouvrir", ["Open", "Abrir", "Öffnen", "打开", "開く"]),
    ("Supprimer", ["Delete", "Eliminar", "Löschen", "删除", "削除"]),
    ("{} plans · {} images · {} s", ["{} shots · {} frames · {} s", "{} planos · {} imágenes · {} s", "{} Einstellungen · {} Bilder · {} s", "{} 个镜头 · {} 帧 · {} 秒", "{} カット · {} コマ · {} 秒"]),
    // camera
    ("Caméra", ["Camera", "Cámara", "Kamera", "摄像机", "カメラ"]),
    ("Voir à travers la caméra", ["View through the camera", "Ver a través de la cámara", "Durch die Kamera sehen", "通过摄像机查看", "カメラを通して見る"]),
    ("Caméra active (exports à travers la caméra)", ["Camera on (exports go through the camera)", "Cámara activa (las exportaciones pasan por la cámara)", "Kamera an (Exporte durch die Kamera)", "启用摄像机（导出通过摄像机）", "カメラをオン（書き出しはカメラ経由）"]),
    ("Glissez sur la toile pour déplacer la caméra à cette image (une clé est créée).", ["Drag on the canvas to move the camera at this frame (a key is made).", "Arrastra en el lienzo para mover la cámara en esta imagen (se crea una clave).", "Auf der Leinwand ziehen, um die Kamera in diesem Bild zu bewegen (ein Schlüssel wird erstellt).", "在画布上拖动以在此帧移动摄像机（会创建关键帧）。", "キャンバスをドラッグしてこのコマのカメラを動かします（キーが作られます）。"]),
    ("Zoom", ["Zoom", "Zoom", "Zoom", "缩放", "ズーム"]),
    ("Clé à cette image", ["Key at this frame", "Clave en esta imagen", "Schlüssel in diesem Bild", "在此帧设关键帧", "このコマにキー"]),
    ("Supprimer la clé", ["Delete key", "Borrar la clave", "Schlüssel löschen", "删除关键帧", "キーを削除"]),
    ("Recentrer", ["Recentre", "Recentrar", "Zentrieren", "重新居中", "中央に戻す"]),
    ("Courbe", ["Curve", "Curva", "Kurve", "曲线", "カーブ"]),
    ("Ralentir au départ et à l'arrivée", ["Slow in and slow out", "Frenar al inicio y al final", "Langsam an- und auslaufen", "开始和结束时减速", "出だしと終わりをゆっくり"]),
    ("Clés : {}", ["Keys: {}", "Claves: {}", "Schlüssel: {}", "关键帧：{}", "キー：{}"]),
    // effects
    ("Effets", ["Effects", "Efectos", "Effekte", "效果", "エフェクト"]),
    ("Flou", ["Blur", "Desenfoque", "Weichzeichner", "模糊", "ぼかし"]),
    ("Mosaïque", ["Mosaic", "Mosaico", "Mosaik", "马赛克", "モザイク"]),
    ("Bruit", ["Noise", "Ruido", "Rauschen", "噪点", "ノイズ"]),
    ("Vague", ["Wave", "Onda", "Welle", "波浪", "波形"]),
    ("Luminosité / contraste", ["Brightness / contrast", "Brillo / contraste", "Helligkeit / Kontrast", "亮度 / 对比度", "明るさ / コントラスト"]),
    ("Teinte / saturation", ["Hue / saturation", "Tono / saturación", "Farbton / Sättigung", "色相 / 饱和度", "色相 / 彩度"]),
    ("Négatif", ["Invert", "Negativo", "Negativ", "反相", "階調の反転"]),
    ("Incrustation couleur", ["Colour key", "Clave de color", "Farbschlüssel", "颜色抠像", "カラーキー"]),
    ("Incrustation luminance", ["Luma key", "Clave de luminancia", "Luminanzschlüssel", "亮度抠像", "ルミナンスキー"]),
    ("Rien à modifier ici : dessinez d'abord.", ["Nothing to change here: draw something first.", "Nada que cambiar aquí: dibuja algo primero.", "Hier gibt es nichts zu ändern: zuerst zeichnen.", "这里没有可修改的内容：请先绘画。", "ここには変更するものがありません：先に描いてください。"]),
    ("Rayon horizontal", ["Horizontal radius", "Radio horizontal", "Horizontaler Radius", "水平半径", "水平半径"]),
    ("Rayon vertical (0 = pareil)", ["Vertical radius (0 = same)", "Radio vertical (0 = igual)", "Vertikaler Radius (0 = gleich)", "垂直半径（0 = 相同）", "垂直半径（0 = 同じ）"]),
    ("Largeur des carreaux", ["Tile width", "Ancho de las teselas", "Kachelbreite", "格子宽度", "タイルの幅"]),
    ("Hauteur des carreaux", ["Tile height", "Alto de las teselas", "Kachelhöhe", "格子高度", "タイルの高さ"]),
    ("Quantité", ["Amount", "Cantidad", "Menge", "数量", "量"]),
    ("Monochrome", ["Monochrome", "Monocromo", "Einfarbig", "单色", "モノクロ"]),
    ("Amplitude horizontale", ["Horizontal amplitude", "Amplitud horizontal", "Horizontale Amplitude", "水平振幅", "水平振幅"]),
    ("Longueur d'onde horizontale", ["Horizontal wavelength", "Longitud de onda horizontal", "Horizontale Wellenlänge", "水平波长", "水平波長"]),
    ("Amplitude verticale", ["Vertical amplitude", "Amplitud vertical", "Vertikale Amplitude", "垂直振幅", "垂直振幅"]),
    ("Longueur d'onde verticale", ["Vertical wavelength", "Longitud de onda vertical", "Vertikale Wellenlänge", "垂直波长", "垂直波長"]),
    ("Phase", ["Phase", "Fase", "Phase", "相位", "位相"]),
    ("Luminosité", ["Brightness", "Brillo", "Helligkeit", "亮度", "明るさ"]),
    ("Contraste", ["Contrast", "Contraste", "Kontrast", "对比度", "コントラスト"]),
    ("Teinte", ["Hue", "Tono", "Farbton", "色相", "色相"]),
    ("Saturation", ["Saturation", "Saturación", "Sättigung", "饱和度", "彩度"]),
    ("Clarté", ["Lightness", "Luminosidad", "Helligkeit", "明度", "明度"]),
    ("Inverse les couleurs.", ["Inverts the colours.", "Invierte los colores.", "Kehrt die Farben um.", "反转颜色。", "色を反転します。"]),
    ("Couleur à rendre transparente", ["Colour to make transparent", "Color que se vuelve transparente", "Farbe, die transparent wird", "要变透明的颜色", "透明にする色"]),
    ("Couleur A", ["Colour A", "Color A", "Farbe A", "颜色 A", "色 A"]),
    ("Seuil", ["Threshold", "Umbral", "Schwelle", "阈值", "しきい値"]),
    ("Rendre transparent le sombre (sinon le clair)", ["Make the dark parts transparent (otherwise the light ones)", "Volver transparente lo oscuro (si no, lo claro)", "Dunkles transparent machen (sonst Helles)", "让暗部透明（否则为亮部）", "暗い部分を透明に（でなければ明るい部分）"]),
    ("Toutes les images du calque", ["Every drawing of the layer", "Todas las imágenes de la capa", "Alle Bilder der Ebene", "该图层的所有画面", "レイヤーのすべての絵"]),
    ("Seulement dans la sélection.", ["Only inside the selection.", "Solo dentro de la selección.", "Nur innerhalb der Auswahl.", "仅在选区内。", "選択範囲の中だけ。"]),
    ("Appliquer", ["Apply", "Aplicar", "Anwenden", "应用", "適用"]),
    ("Déformation", ["Warp", "Deformar", "Verformen", "变形", "ワープ"]),
    ("Lueur", ["Glow", "Resplandor", "Leuchten", "发光", "光彩"]),
    ("Lettres creuses", ["Hollow letters", "Letras huecas", "Hohle Buchstaben", "空心字", "中抜き文字"]),
    // custom brushes and presets
    ("Pinceau personnalisé", ["Custom brush", "Pincel personalizado", "Eigener Pinsel", "自定义画笔", "カスタムブラシ"]),
    ("Pas encore de pinceau : sélectionnez une zone puis « Créer un pinceau avec la sélection », ou chargez une image.", ["No brush yet: select an area then “Make a brush from the selection”, or load a picture.", "Aún no hay pincel: selecciona una zona y luego «Crear un pincel con la selección», o carga una imagen.", "Noch kein Pinsel: einen Bereich auswählen, dann „Pinsel aus der Auswahl erstellen“, oder ein Bild laden.", "还没有画笔：先选择一个区域，再点“用选区创建画笔”，或载入一张图像。", "ブラシがまだありません：範囲を選択して「選択範囲からブラシを作成」、または画像を読み込みます。"]),
    ("Créer un pinceau avec la sélection", ["Make a brush from the selection", "Crear un pincel con la selección", "Pinsel aus der Auswahl erstellen", "用选区创建画笔", "選択範囲からブラシを作成"]),
    ("Depuis la sélection", ["From the selection", "Desde la selección", "Aus der Auswahl", "来自选区", "選択範囲から"]),
    ("Charger une image…", ["Load a picture…", "Cargar una imagen…", "Bild laden…", "载入图像…", "画像を読み込む…"]),
    ("La sélection est vide.", ["The selection is empty.", "La selección está vacía.", "Die Auswahl ist leer.", "选区是空的。", "選択範囲が空です。"]),
    ("Pinceau créé : {} × {} px", ["Brush made: {} × {} px", "Pincel creado: {} × {} px", "Pinsel erstellt: {} × {} px", "已创建画笔：{} × {} px", "ブラシを作成しました：{} × {} px"]),
    ("Espacement", ["Spacing", "Espaciado", "Abstand", "间距", "間隔"]),
    ("Peindre avec la couleur A (garder la forme)", ["Paint with colour A (keep the shape)", "Pintar con el color A (mantener la forma)", "Mit Farbe A malen (Form behalten)", "用颜色 A 绘制（保留形状）", "色Aで描く（形を保つ）"]),
    ("Préréglages", ["Presets", "Ajustes guardados", "Voreinstellungen", "预设", "プリセット"]),
    ("Alt + clic droit : supprimer le préréglage", ["Alt + right-click: delete the preset", "Alt + clic derecho: borrar el ajuste", "Alt + Rechtsklick: Voreinstellung löschen", "Alt + 右键：删除预设", "Alt + 右クリック：プリセットを削除"]),
    ("Nom", ["Name", "Nombre", "Name", "名称", "名前"]),
    // palette and import
    ("Alt + clic droit : retirer de la palette", ["Alt + right-click: remove from the palette", "Alt + clic derecho: quitar de la paleta", "Alt + Rechtsklick: aus der Palette entfernen", "Alt + 右键：从调色板移除", "Alt + 右クリック：パレットから削除"]),
    ("+ Ajouter la couleur", ["+ Add colour", "+ Añadir el color", "+ Farbe hinzufügen", "+ 添加颜色", "+ 色を追加"]),
    ("Charger…", ["Load…", "Cargar…", "Laden…", "载入…", "読み込む…"]),
    ("Enregistrer…", ["Save…", "Guardar…", "Speichern…", "保存…", "保存…"]),
    ("Palette", ["Palette", "Paleta", "Palette", "调色板", "パレット"]),
    ("Échec de l'ouverture : {}", ["Could not open: {}", "No se pudo abrir: {}", "Öffnen fehlgeschlagen: {}", "无法打开：{}", "開けませんでした：{}"]),
    ("Importer une image (PNG, JPEG)…", ["Import a picture (PNG, JPEG)…", "Importar una imagen (PNG, JPEG)…", "Bild importieren (PNG, JPEG)…", "导入图像（PNG、JPEG）…", "画像を読み込む（PNG、JPEG）…"]),
    ("Importer une suite d'images (nouveau calque)…", ["Import a picture sequence (new layer)…", "Importar una secuencia de imágenes (nueva capa)…", "Bildfolge importieren (neue Ebene)…", "导入图像序列（新图层）…", "連番画像を読み込む（新規レイヤー）…"]),
    ("Image importée : déplacez-la, puis Entrée pour valider.", ["Picture imported: move it, then press Enter to apply.", "Imagen importada: muévela y pulsa Intro para aplicar.", "Bild importiert: verschieben, dann Eingabe zum Anwenden.", "已导入图像：移动它，然后按回车应用。", "画像を読み込みました：移動して Enter で確定。"]),
    ("{} images importées dans un nouveau calque.", ["{} pictures imported into a new layer.", "{} imágenes importadas en una nueva capa.", "{} Bilder in eine neue Ebene importiert.", "已将 {} 张图像导入新图层。", "{} 枚の画像を新規レイヤーに読み込みました。"]),
    // timeline zoom, image marks, notes, in-betweens
    ("Timecode", ["Timecode", "Código de tiempo", "Timecode", "时间码", "タイムコード"]),
    ("La règle affiche le temps (secondes:images) au lieu des numéros d'image", ["The ruler shows time (seconds:frames) instead of frame numbers", "La regla muestra el tiempo (segundos:imágenes) en lugar de los números de imagen", "Das Lineal zeigt die Zeit (Sekunden:Bilder) statt der Bildnummern", "标尺显示时间（秒:帧）而不是帧编号", "ルーラーにコマ番号の代わりに時間（秒:コマ）を表示"]),
    ("Ajuster", ["Fit", "Ajustar", "Anpassen", "适应", "合わせる"]),
    ("Tout le plan", ["The whole shot", "Todo el plano", "Die ganze Einstellung", "整个镜头", "カット全体"]),
    ("Les cellules choisies", ["The selected cells", "Las celdas elegidas", "Die gewählten Zellen", "所选单元格", "選択したセル"]),
    ("Le son", ["The sound", "El sonido", "Den Ton", "声音", "サウンド"]),
    ("Taille normale", ["Normal size", "Tamaño normal", "Normale Größe", "正常大小", "標準サイズ"]),
    ("Ajuster le zoom de la timeline", ["Fit the timeline's zoom", "Ajustar el zoom de la línea de tiempo", "Zoom der Zeitleiste anpassen", "调整时间轴缩放", "タイムラインのズームを合わせる"]),
    ("Zoom avant de la timeline", ["Zoom the timeline in", "Acercar la línea de tiempo", "Zeitleiste vergrößern", "放大时间轴", "タイムラインを拡大"]),
    ("Zoom arrière de la timeline", ["Zoom the timeline out", "Alejar la línea de tiempo", "Zeitleiste verkleinern", "缩小时间轴", "タイムラインを縮小"]),
    ("Zoom de la timeline (aussi : Ctrl + molette)", ["Timeline zoom (also: Ctrl + wheel)", "Zoom de la línea de tiempo (también: Ctrl + rueda)", "Zoom der Zeitleiste (auch: Strg + Mausrad)", "时间轴缩放（也可：Ctrl + 滚轮）", "タイムラインのズーム（Ctrl + ホイールでも可）"]),
    ("Timeline : voir tout le plan", ["Timeline: show the whole shot", "Línea de tiempo: ver todo el plano", "Zeitleiste: ganze Einstellung zeigen", "时间轴：显示整个镜头", "タイムライン：カット全体を表示"]),
    ("Timeline : images ou timecode", ["Timeline: frames or timecode", "Línea de tiempo: imágenes o código de tiempo", "Zeitleiste: Bilder oder Timecode", "时间轴：帧或时间码", "タイムライン：コマまたはタイムコード"]),
    ("Notes", ["Notes", "Notas", "Notizen", "备注", "メモ"]),
    ("Note ici…", ["Note here…", "Nota aquí…", "Notiz hier…", "在此添加备注…", "ここにメモ…"]),
    ("Note précédente", ["Previous note", "Nota anterior", "Vorherige Notiz", "上一个备注", "前のメモ"]),
    ("Note suivante", ["Next note", "Nota siguiente", "Nächste Notiz", "下一个备注", "次のメモ"]),
    ("Afficher les notes", ["Show the notes", "Mostrar las notas", "Notizen anzeigen", "显示备注", "メモを表示"]),
    ("Note de l'image {}", ["Note at frame {}", "Nota de la imagen {}", "Notiz bei Bild {}", "第 {} 帧的备注", "コマ {} のメモ"]),
    ("Valider", ["OK", "Aceptar", "OK", "确定", "OK"]),
    ("Retirer la note", ["Remove the note", "Quitar la nota", "Notiz entfernen", "移除备注", "メモを削除"]),
    ("Marque de l'image", ["Image mark", "Marca de la imagen", "Bildmarke", "图像标记", "画像マーク"]),
    ("Retirer la marque", ["Remove the mark", "Quitar la marca", "Marke entfernen", "移除标记", "マークを外す"]),
    ("Marquer l'image (ajouter / retirer)", ["Mark the image (add / remove)", "Marcar la imagen (añadir / quitar)", "Bild markieren (hinzufügen / entfernen)", "标记图像（添加 / 移除）", "画像をマーク（追加 / 削除）"]),
    ("Image marquée précédente", ["Previous marked image", "Imagen marcada anterior", "Vorheriges markiertes Bild", "上一个已标记图像", "前のマーク付き画像"]),
    ("Image marquée suivante", ["Next marked image", "Imagen marcada siguiente", "Nächstes markiertes Bild", "下一个已标记图像", "次のマーク付き画像"]),
    ("Pas de dessin à marquer ici (ou calque verrouillé).", ["No drawing to mark here (or the layer is locked).", "No hay dibujo que marcar aquí (o la capa está bloqueada).", "Hier gibt es keine Zeichnung zum Markieren (oder die Ebene ist gesperrt).", "此处没有可标记的画面（或图层已锁定）。", "ここにはマークできる絵がありません（またはレイヤーがロック中）。"]),
    ("Pas d'autre image marquée de ce côté sur ce calque.", ["No other marked image that way on this layer.", "No hay otra imagen marcada en ese sentido en esta capa.", "In dieser Richtung gibt es auf dieser Ebene kein weiteres markiertes Bild.", "此图层在该方向上没有其他已标记图像。", "このレイヤーのその方向には他にマーク付き画像がありません。"]),
    ("Insérer des intervalles", ["Insert in-betweens", "Insertar intercalados", "Zwischenbilder einfügen", "插入中间画", "中割りを挿入"]),
    ("Ajoute ce nombre de dessins entre ce dessin et le suivant du calque.", ["Adds this many drawings between this drawing and the layer's next one.", "Añade este número de dibujos entre este dibujo y el siguiente de la capa.", "Fügt so viele Zeichnungen zwischen dieser und der nächsten Zeichnung der Ebene ein.", "在此画面与图层的下一张画面之间加入这么多张画面。", "この絵とレイヤーの次の絵の間に、この枚数の絵を追加します。"]),
    ("en fondu", ["cross-faded", "en fundido", "überblendet", "交叉淡化", "クロスフェード"]),
    ("Chaque intervalle est un mélange des deux dessins qui l'entourent, pour dessiner par-dessus ; sinon il est vide.", ["Each in-between is a mix of the two drawings around it, to draw over; otherwise it is blank.", "Cada intercalado es una mezcla de los dos dibujos que lo rodean, para dibujar encima; si no, queda vacío.", "Jedes Zwischenbild ist eine Mischung der beiden Zeichnungen davor und danach, zum Überzeichnen; sonst ist es leer.", "每张中间画是前后两张画面的混合，可在其上绘制；否则为空白。", "各中割りは前後2枚の絵を混ぜたもので、その上に描けます。オフなら白紙です。"]),
    ("{} intervalles insérés.", ["{} in-betweens inserted.", "{} intercalados insertados.", "{} Zwischenbilder eingefügt.", "已插入 {} 张中间画。", "中割りを {} 枚挿入しました。"]),
    ("Il faut un dessin après celui-ci sur le calque (non verrouillé).", ["The layer needs a drawing after this one (and must not be locked).", "Hace falta un dibujo después de este en la capa (no bloqueada).", "Die Ebene braucht eine Zeichnung nach dieser (und darf nicht gesperrt sein).", "图层在此画面之后需要有一张画面（且未锁定）。", "このレイヤーにはこの絵の後に絵が必要です（ロックされていないこと）。"]),
    // guides and flipping
    ("Grille", ["Grid", "Cuadrícula", "Raster", "网格", "グリッド"]),
    ("Pas de la grille", ["Grid spacing", "Paso de la cuadrícula", "Rasterabstand", "网格间距", "グリッド間隔"]),
    ("Règle des tiers", ["Rule of thirds", "Regla de los tercios", "Drittelregel", "三分法", "三分割法"]),
    ("Centre et zone de sécurité", ["Centre and safe area", "Centro y zona segura", "Mitte und Sicherheitsbereich", "中心与安全区", "中心とセーフエリア"]),
    ("feuilleter (maintenir)", ["flip (hold)", "hojear (mantener)", "blättern (halten)", "翻页（按住）", "パラパラ（長押し）"]),
    // exposure sheet
    ("Feuille d'exposition", ["Exposure sheet", "Hoja de exposición", "Belichtungsblatt", "摄影表", "タイムシート"]),
    ("Une ligne par image, une colonne par calque : le numéro du dessin quand il commence, un trait tant qu'il est tenu.", ["One row per frame, one column per layer: the drawing's number where it starts, a line while it is held.", "Una fila por imagen, una columna por capa: el número del dibujo donde empieza, una línea mientras se mantiene.", "Eine Zeile pro Bild, eine Spalte pro Ebene: die Nummer der Zeichnung, wo sie beginnt, ein Strich, solange sie gehalten wird.", "每帧一行，每个图层一列：画面开始处显示编号，持续期间显示竖线。", "1行が1コマ、1列が1レイヤー：絵の始まりに番号、続く間は線。"]),
    // AI / MCP
    ("Aide", ["Help", "Ayuda", "Hilfe", "帮助", "ヘルプ"]),
    ("Connecter une IA (MCP)…", ["Connect an AI (MCP)…", "Conectar una IA (MCP)…", "KI verbinden (MCP)…", "连接 AI（MCP）…", "AIを接続（MCP）…"]),
    ("Connecter une IA (MCP)", ["Connect an AI (MCP)", "Conectar una IA (MCP)", "KI verbinden (MCP)", "连接 AI（MCP）", "AIを接続（MCP）"]),
    ("Toonlight peut être piloté par un assistant IA compatible MCP (Claude, Cursor, ChatGPT et d'autres) : créer des calques et des images, dessiner, remplir, exporter, et regarder le résultat.", ["Toonlight can be driven by an MCP-compatible AI assistant (Claude, Cursor, ChatGPT and others): create layers and frames, draw, fill, export, and look at the result.", "Toonlight puede ser controlado por un asistente de IA compatible con MCP (Claude, Cursor, ChatGPT y otros): crear capas e imágenes, dibujar, rellenar, exportar y ver el resultado.", "Toonlight kann von einem MCP-fähigen KI-Assistenten (Claude, Cursor, ChatGPT und andere) gesteuert werden: Ebenen und Bilder anlegen, zeichnen, füllen, exportieren und das Ergebnis ansehen.", "Toonlight 可由兼容 MCP 的 AI 助手（Claude、Cursor、ChatGPT 等）操作：创建图层和帧、绘画、填充、导出并查看结果。", "Toonlight は MCP 対応の AI アシスタント（Claude、Cursor、ChatGPT など）から操作できます：レイヤーや画像の作成、描画、塗りつぶし、書き出し、結果の確認。"]),
    ("Claude Desktop, Cursor et la plupart des applications : ajoutez ceci à leur fichier de configuration MCP", ["Claude Desktop, Cursor and most apps: add this to their MCP configuration file", "Claude Desktop, Cursor y la mayoría de las aplicaciones: añade esto a su archivo de configuración MCP", "Claude Desktop, Cursor und die meisten Apps: dies in ihre MCP-Konfigurationsdatei einfügen", "Claude Desktop、Cursor 及大多数应用：将以下内容加入其 MCP 配置文件", "Claude Desktop、Cursor など多くのアプリ：MCP 設定ファイルにこれを追加"]),
    ("Copier la configuration", ["Copy the configuration", "Copiar la configuración", "Konfiguration kopieren", "复制配置", "設定をコピー"]),
    ("Claude Code : tapez cette commande dans un terminal", ["Claude Code: type this command in a terminal", "Claude Code: escribe este comando en una terminal", "Claude Code: diesen Befehl im Terminal eingeben", "Claude Code：在终端中输入此命令", "Claude Code：ターミナルでこのコマンドを入力"]),
    ("Copier la commande", ["Copy the command", "Copiar el comando", "Befehl kopieren", "复制命令", "コマンドをコピー"]),
    ("Copié.", ["Copied.", "Copiado.", "Kopiert.", "已复制。", "コピーしました。"]),
    ("Pour voir le travail de l'IA en direct : enregistrez votre projet ici, puis demandez-lui d'ouvrir et d'enregistrer ce même fichier. La fenêtre le recharge dès qu'il change (si vous n'avez pas de changements non enregistrés).", ["To watch the AI work live: save your project here, then ask it to open and save that same file. The window reloads it as soon as it changes (if you have no unsaved changes).", "Para ver el trabajo de la IA en directo: guarda tu proyecto aquí y pídele que abra y guarde ese mismo archivo. La ventana lo recarga en cuanto cambia (si no tienes cambios sin guardar).", "Um der KI live zuzusehen: Projekt hier speichern und sie bitten, dieselbe Datei zu öffnen und zu speichern. Das Fenster lädt sie neu, sobald sie sich ändert (wenn keine ungespeicherten Änderungen vorliegen).", "实时查看 AI 的工作：先在这里保存项目，再让 AI 打开并保存同一个文件。文件一变化窗口就会重新加载（若您没有未保存的更改）。", "AI の作業をリアルタイムで見るには：ここでプロジェクトを保存し、AI に同じファイルを開いて保存するよう頼みます。ファイルが変わるとウィンドウが再読み込みします（未保存の変更がない場合）。"]),
    ("Fichier ouvert :", ["Open file:", "Archivo abierto:", "Geöffnete Datei:", "已打开的文件：", "開いているファイル："]),
    ("Le fichier a été modifié par un autre programme ; vos changements non enregistrés sont gardés ici.", ["The file was changed by another program; your unsaved changes are kept here.", "Otro programa modificó el archivo; tus cambios sin guardar se mantienen aquí.", "Die Datei wurde von einem anderen Programm geändert; Ihre ungespeicherten Änderungen bleiben hier erhalten.", "文件已被其他程序修改；您未保存的更改仍保留在此。", "別のプログラムがファイルを変更しました。未保存の変更はここに残っています。"]),
    ("Projet rechargé : il a été modifié par un autre programme.", ["Project reloaded: it was changed by another program.", "Proyecto recargado: otro programa lo modificó.", "Projekt neu geladen: es wurde von einem anderen Programm geändert.", "项目已重新加载：它被其他程序修改过。", "プロジェクトを再読み込みしました：別のプログラムが変更しました。"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `tr("…")` / `trf("…", …)` text in the code has a row in the table.
    #[test]
    fn every_text_is_translated() {
        let sources = [
            include_str!("app.rs"),
            include_str!("brush.rs"),
            include_str!("doc.rs"),
            include_str!("io.rs"),
            include_str!("fx.rs"),
            include_str!("custom.rs"),
            include_str!("select.rs"),
            include_str!("text.rs"),
            include_str!("sound.rs"),
            include_str!("keymap.rs"),
            include_str!("settings.rs"),
            include_str!("camera.rs"),
            include_str!("layout.rs"),
            include_str!("prefs.rs"),
            include_str!("psd.rs"),
        ];
        let mut missing = vec![];
        for src in sources {
            for marker in ["tr(\"", "trf(\""] {
                for (i, _) in src.match_indices(marker) {
                    // skip identifiers ending in tr / trf (e.g. `attr("`)
                    if i > 0 && src.as_bytes()[i - 1].is_ascii_alphanumeric() {
                        continue;
                    }
                    let rest = &src[i + marker.len()..];
                    let end = rest.find('"').unwrap();
                    let key = &rest[..end];
                    if !map().contains_key(key) {
                        missing.push(key.to_string());
                    }
                }
            }
        }
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }

    #[test]
    fn names_and_codes() {
        assert_eq!(Lang::from_code("french"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("japanese.loc"), Some(Lang::Ja));
        assert_eq!(Lang::from_code("zh_CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("de-DE"), Some(Lang::De));
        assert_eq!(Lang::from_code("pt-BR"), None);
    }

    #[test]
    fn templates_fill_in_order() {
        set_lang(Lang::En);
        assert_eq!(trf("Taille : {} × {}", &[&1920, &1080]), "Size: 1920 × 1080");
        set_lang(Lang::Fr);
        assert_eq!(trf("Taille : {} × {}", &[&1920, &1080]), "Taille : 1920 × 1080");
        for (k, row) in TABLE {
            let n = k.matches("{}").count();
            for t in row {
                assert_eq!(t.matches("{}").count(), n, "placeholders differ for {k}");
            }
        }
    }
}
