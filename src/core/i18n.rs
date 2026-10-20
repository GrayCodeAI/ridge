//! i18n internationalization (from Luvus).
//! Supports 8 languages: en, es, fr, de, ja, zh, ko, ru.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "zh")]
    Chinese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "ru")]
    Russian,
}

impl Language {
    pub const ALL: [Language; 8] = [
        Language::English,
        Language::Spanish,
        Language::French,
        Language::German,
        Language::Japanese,
        Language::Chinese,
        Language::Korean,
        Language::Russian,
    ];

    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
            Language::French => "fr",
            Language::German => "de",
            Language::Japanese => "ja",
            Language::Chinese => "zh",
            Language::Korean => "ko",
            Language::Russian => "ru",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Español",
            Language::French => "Français",
            Language::German => "Deutsch",
            Language::Japanese => "日本語",
            Language::Chinese => "中文",
            Language::Korean => "한국어",
            Language::Russian => "Русский",
        }
    }
}

/// Translation database: key -> (language -> translation).
type Translations = HashMap<&'static str, HashMap<Language, &'static str>>;

/// Global translation table.
fn translations() -> Translations {
    let mut m: Translations = HashMap::new();

    macro_rules! tr {
        ($key:expr, $($lang:expr => $text:expr),* $(,)?) => {
            {
                let mut hm = HashMap::new();
                $(
                    hm.insert($lang, $text);
                )*
                m.insert($key, hm);
            }
        };
    }

    tr!("quit", Language::English => "quit", Language::Spanish => "salir", Language::French => "quitter", Language::German => "beenden", Language::Japanese => "終了", Language::Chinese => "退出", Language::Korean => "종료", Language::Russian => "выход");
    tr!("spawn", Language::English => "spawn", Language::Spanish => "generar", Language::French => "générer", Language::German => "erzeugen", Language::Japanese => "生成", Language::Chinese => "生成", Language::Korean => "생성", Language::Russian => "создать");
    tr!("pane", Language::English => "pane", Language::Spanish => "panel", Language::French => "volet", Language::German => "Bereich", Language::Japanese => "パネル", Language::Chinese => "窗格", Language::Korean => "창", Language::Russian => "панель");
    tr!("blocked", Language::English => "blocked", Language::Spanish => "bloqueado", Language::French => "bloqué", Language::German => "blockiert", Language::Japanese => "ブロック済み", Language::Chinese => "阻塞", Language::Korean => "차단됨", Language::Russian => "заблокировано");
    tr!("working", Language::English => "working", Language::Spanish => "trabajando", Language::French => "en cours", Language::German => "arbeitet", Language::Japanese => "実行中", Language::Chinese => "工作中", Language::Korean => "작업 중", Language::Russian => "работает");
    tr!("done", Language::English => "done", Language::Spanish => "hecho", Language::French => "terminé", Language::German => "erledigt", Language::Japanese => "完了", Language::Chinese => "完成", Language::Korean => "완료", Language::Russian => "готово");
    tr!("idle", Language::English => "idle", Language::Spanish => "inactivo", Language::French => "inactif", Language::German => "leerlauf", Language::Japanese => "アイドル", Language::Chinese => "空闲", Language::Korean => "대기 중", Language::Russian => "простаивает");
    tr!("agents", Language::English => "agents", Language::Spanish => "agentes", Language::French => "agents", Language::German => "Agenten", Language::Japanese => "エージェント", Language::Chinese => "代理", Language::Korean => "에이전트", Language::Russian => "агенты");
    tr!("git", Language::English => "git", Language::Spanish => "git", Language::French => "git", Language::German => "git", Language::Japanese => "ギット", Language::Chinese => "Git", Language::Korean => "깃", Language::Russian => "гит");
    tr!("diff", Language::English => "diff", Language::Spanish => "diferencia", Language::French => "différence", Language::German => "Unterschied", Language::Japanese => "差分", Language::Chinese => "差异", Language::Korean => "차이", Language::Russian => "разница");
    tr!("files", Language::English => "files", Language::Spanish => "archivos", Language::French => "fichiers", Language::German => "Dateien", Language::Japanese => "ファイル", Language::Chinese => "文件", Language::Korean => "파일", Language::Russian => "файлы");
    tr!("orchestration", Language::English => "orchestration", Language::Spanish => "orquestación", Language::French => "orchestration", Language::German => "Orchestrierung", Language::Japanese => "オーケストレーション", Language::Chinese => "编排", Language::Korean => "오케스트레이션", Language::Russian => "оркестровка");
    tr!("settings", Language::English => "settings", Language::Spanish => "configuración", Language::French => "paramètres", Language::German => "Einstellungen", Language::Japanese => "設定", Language::Chinese => "设置", Language::Korean => "설정", Language::Russian => "настройки");
    tr!("dashboard", Language::English => "dashboard", Language::Spanish => "panel", Language::French => "tableau de bord", Language::German => "Dashboard", Language::Japanese => "ダッシュボード", Language::Chinese => "仪表板", Language::Korean => "대시보드", Language::Russian => "панель управления");
    tr!("race", Language::English => "race", Language::Spanish => "carrera", Language::French => "course", Language::German => "Rennen", Language::Japanese => "レース", Language::Chinese => "比赛", Language::Korean => "경주", Language::Russian => "гонка");
    tr!("inbox", Language::English => "inbox", Language::Spanish => "bandeja de entrada", Language::French => "boîte de réception", Language::German => "Posteingang", Language::Japanese => "受信箱", Language::Chinese => "收件箱", Language::Korean => "받은 편지함", Language::Russian => "входящие");
    tr!("queue", Language::English => "queue", Language::Spanish => "cola", Language::French => "file d'attente", Language::German => "Warteschlange", Language::Japanese => "キュー", Language::Chinese => "队列", Language::Korean => "큐", Language::Russian => "очередь");
    tr!("help", Language::English => "help", Language::Spanish => "ayuda", Language::French => "aide", Language::German => "Hilfe", Language::Japanese => "ヘルプ", Language::Chinese => "帮助", Language::Korean => "도움말", Language::Russian => "помощь");
    tr!("copy", Language::English => "copy", Language::Spanish => "copiar", Language::French => "copier", Language::German => "kopieren", Language::Japanese => "コピー", Language::Chinese => "复制", Language::Korean => "복사", Language::Russian => "копировать");
    tr!("paste", Language::English => "paste", Language::Spanish => "pegar", Language::French => "coller", Language::German => "einfügen", Language::Japanese => "貼り付け", Language::Chinese => "粘贴", Language::Korean => "붙여넣기", Language::Russian => "вставить");
    tr!("update", Language::English => "update", Language::Spanish => "actualizar", Language::French => "mettre à jour", Language::German => "aktualisieren", Language::Japanese => "更新", Language::Chinese => "更新", Language::Korean => "업데이트", Language::Russian => "обновить");
    tr!("machine", Language::English => "machine", Language::Spanish => "máquina", Language::French => "machine", Language::German => "Maschine", Language::Japanese => "マシン", Language::Chinese => "机器", Language::Korean => "기계", Language::Russian => "машина");
    tr!("fork", Language::English => "fork", Language::Spanish => "bifurcar", Language::French => "fourchette", Language::German => "Gabeln", Language::Japanese => "フォーク", Language::Chinese => "分叉", Language::Korean => "포크", Language::Russian => "вилка");
    tr!("commander", Language::English => "commander", Language::Spanish => "comandante", Language::French => "commandant", Language::German => "Kommandant", Language::Japanese => "指揮官", Language::Chinese => "指挥官", Language::Korean => "지휘관", Language::Russian => "командир");

    m
}

/// Translate a key into the given language.
pub fn t(key: &str, lang: Language) -> String {
    translations()
        .get(key)
        .and_then(|hm| hm.get(&lang))
        .copied()
        .unwrap_or(key)
        .to_string()
}

/// Translate a key into the current app language.
pub fn t_app(key: &str, lang: &Language) -> String {
    t(key, *lang)
}

impl FromStr for Language {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "en" => Ok(Language::English),
            "es" => Ok(Language::Spanish),
            "fr" => Ok(Language::French),
            "de" => Ok(Language::German),
            "ja" => Ok(Language::Japanese),
            "zh" => Ok(Language::Chinese),
            "ko" => Ok(Language::Korean),
            "ru" => Ok(Language::Russian),
            _ => Err(format!("unknown language: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_english() {
        assert_eq!(t("quit", Language::English), "quit");
    }

    #[test]
    fn t_spanish() {
        assert_eq!(t("quit", Language::Spanish), "salir");
    }

    #[test]
    fn t_japanese() {
        assert_eq!(t("agents", Language::Japanese), "エージェント");
    }

    #[test]
    fn t_fallback_to_key() {
        assert_eq!(t("nonexistent_key", Language::French), "nonexistent_key");
    }

    #[test]
    fn language_codes() {
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::Spanish.code(), "es");
        assert_eq!(Language::Japanese.code(), "ja");
        assert_eq!(Language::Chinese.code(), "zh");
    }

    #[test]
    fn all_languages_count() {
        assert_eq!(Language::ALL.len(), 8);
    }
}
