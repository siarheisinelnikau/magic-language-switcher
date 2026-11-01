//! Interface preferences are independent of the keyboard layouts being converted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Russian,
    French,
    German,
    Spanish,
    Italian,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn code(self) -> &'static str {
        ["en", "ru", "fr", "de", "es", "it"][self as usize]
    }
    pub fn native_name(self) -> &'static str {
        [
            "English",
            "Русский",
            "Français",
            "Deutsch",
            "Español",
            "Italiano",
        ][self as usize]
    }
    pub fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == value)
            .unwrap_or_default()
    }
    pub fn text(self, source: &str) -> &str {
        if self == Self::English {
            return source;
        }
        MESSAGES
            .iter()
            .find(|(key, _)| *key == source)
            .map_or(source, |(_, translations)| translations[self as usize - 1])
    }
    pub fn format(self, source: &str, values: &[(&str, &str)]) -> String {
        let mut template = self.text(source);
        let mut text = String::with_capacity(template.len());
        while let Some(open) = template.find('{') {
            text.push_str(&template[..open]);
            template = &template[open..];
            let Some(close) = template.find('}') else {
                break;
            };
            let name = &template[1..close];
            text.push_str(
                values
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map_or(&template[..=close], |(_, value)| *value),
            );
            template = &template[close + 1..];
        }
        text.push_str(template);
        text
    }
    /// Only known application messages are translated; arbitrary editor data is untouched.
    pub fn message(self, source: &str) -> String {
        let translated = self.text(source);
        if translated != source || self == Self::English {
            return translated.to_owned();
        }
        for (prefix, template) in [
            (
                "Could not save settings: ",
                "Could not save settings: {detail}",
            ),
            (
                "Could not start the update: ",
                "Could not start the update: {detail}",
            ),
            (
                "Update check or download failed: ",
                "Update check or download failed: {detail}",
            ),
            ("Text corrected, but ", "Text corrected, but {detail}"),
        ] {
            if let Some(detail) = source.strip_prefix(prefix) {
                return self.format(template, &[("detail", self.text(detail))]);
            }
        }
        if let Some(shortcut) =
            source.strip_suffix(" is unavailable or already used by another app.")
        {
            return self.format(
                "{shortcut} is unavailable or already used by another app.",
                &[("shortcut", shortcut)],
            );
        }
        source.to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn code(self) -> &'static str {
        if self == Self::Dark { "dark" } else { "light" }
    }
    pub fn parse(value: &str) -> Self {
        if value == "dark" {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

// Every entry has all five translations; English is the complete source and fallback.
const MESSAGES: &[(&str, [&str; 5])] = &[
    (
        "Language",
        ["Язык интерфейса", "Langue", "Sprache", "Idioma", "Lingua"],
    ),
    ("Theme", ["Тема", "Thème", "Darstellung", "Tema", "Tema"]),
    ("Light", ["Светлая", "Clair", "Hell", "Claro", "Chiaro"]),
    ("Dark", ["Тёмная", "Sombre", "Dunkel", "Oscuro", "Scuro"]),
    (
        "Selection first; otherwise the last word.",
        [
            "Выделенный текст или последнее слово.",
            "La sélection, sinon le dernier mot.",
            "Auswahl, sonst das letzte Wort.",
            "La selección o la última palabra.",
            "La selezione o l’ultima parola.",
        ],
    ),
    (
        "●  Ready",
        ["●  Готово", "●  Prêt", "●  Bereit", "●  Listo", "●  Pronto"],
    ),
    (
        "●  Paused",
        [
            "●  Приостановлено",
            "●  En pause",
            "●  Pausiert",
            "●  En pausa",
            "●  In pausa",
        ],
    ),
    (
        "●  Set shortcut",
        [
            "●  Задайте сочетание",
            "●  Définir le raccourci",
            "●  Kürzel festlegen",
            "●  Definir atajo",
            "●  Imposta scorciatoia",
        ],
    ),
    (
        "●  Shortcut conflict",
        [
            "●  Сочетание занято",
            "●  Raccourci occupé",
            "●  Kürzel belegt",
            "●  Atajo ocupado",
            "●  Scorciatoia occupata",
        ],
    ),
    ("Pause", ["Пауза", "Pause", "Pause", "Pausar", "Pausa"]),
    (
        "Resume",
        [
            "Продолжить",
            "Reprendre",
            "Fortsetzen",
            "Reanudar",
            "Riprendi",
        ],
    ),
    ("From", ["Из раскладки", "Depuis", "Von", "De", "Da"]),
    ("To", ["В раскладку", "Vers", "Nach", "A", "A"]),
    (
        "Correct text",
        [
            "Исправить текст",
            "Corriger le texte",
            "Text korrigieren",
            "Corregir texto",
            "Correggi testo",
        ],
    ),
    (
        "Undo last fix",
        [
            "Отменить исправление",
            "Annuler la correction",
            "Korrektur rückgängig",
            "Deshacer corrección",
            "Annulla correzione",
        ],
    ),
    (
        "Try it here",
        [
            "Попробуйте здесь",
            "Essayez ici",
            "Hier ausprobieren",
            "Pruébalo aquí",
            "Prova qui",
        ],
    ),
    (
        "Click a shortcut to change it.",
        [
            "Нажмите сочетание, чтобы изменить его.",
            "Cliquez sur un raccourci pour le modifier.",
            "Zum Ändern auf ein Kürzel klicken.",
            "Haz clic en un atajo para cambiarlo.",
            "Fai clic su una scorciatoia per cambiarla.",
        ],
    ),
    (
        "Start with Windows",
        [
            "Запускать с Windows",
            "Démarrer avec Windows",
            "Mit Windows starten",
            "Iniciar con Windows",
            "Avvia con Windows",
        ],
    ),
    (
        "About",
        [
            "О программе",
            "À propos",
            "Info",
            "Acerca de",
            "Informazioni",
        ],
    ),
    (
        "Auto (active layout)",
        [
            "Авто (активная раскладка)",
            "Auto (disposition active)",
            "Auto (aktives Layout)",
            "Auto (distribución activa)",
            "Auto (layout attivo)",
        ],
    ),
    (
        "Next installed layout",
        [
            "Следующая раскладка",
            "Disposition suivante",
            "Nächstes installiertes Layout",
            "Siguiente distribución",
            "Layout successivo",
        ],
    ),
    ("ready", ["готово", "prêt", "bereit", "listo", "pronto"]),
    (
        "paused",
        [
            "приостановлено",
            "en pause",
            "pausiert",
            "en pausa",
            "in pausa",
        ],
    ),
    (
        "Wait for the current correction to finish.",
        [
            "Дождитесь окончания исправления.",
            "Attendez la fin de la correction.",
            "Warten Sie auf das Ende der Korrektur.",
            "Espera a que termine la corrección.",
            "Attendi la fine della correzione.",
        ],
    ),
    (
        "Press keys...",
        [
            "Нажмите клавиши…",
            "Appuyez sur les touches…",
            "Tasten drücken…",
            "Pulsa las teclas…",
            "Premi i tasti…",
        ],
    ),
    (
        "Press keys · Esc cancels · Backspace resets",
        [
            "Клавиши · Esc — отмена · Backspace — сброс",
            "Touches · Échap annule · Retour efface",
            "Tasten · Esc bricht ab · Rücktaste setzt zurück",
            "Teclas · Esc cancela · Retroceso restablece",
            "Tasti · Esc annulla · Backspace ripristina",
        ],
    ),
    (
        "Shortcut change cancelled.",
        [
            "Изменение сочетания отменено.",
            "Modification du raccourci annulée.",
            "Änderung des Kürzels abgebrochen.",
            "Cambio de atajo cancelado.",
            "Modifica della scorciatoia annullata.",
        ],
    ),
    (
        "Release the keys to save this shortcut.",
        [
            "Отпустите клавиши для сохранения.",
            "Relâchez les touches pour enregistrer.",
            "Zum Speichern die Tasten loslassen.",
            "Suelta las teclas para guardar el atajo.",
            "Rilascia i tasti per salvare.",
        ],
    ),
    (
        "Shortcut saved: {shortcut}",
        [
            "Сохранено: {shortcut}",
            "Raccourci enregistré : {shortcut}",
            "Kürzel gespeichert: {shortcut}",
            "Atajo guardado: {shortcut}",
            "Scorciatoia salvata: {shortcut}",
        ],
    ),
    (
        "{detail} Previous shortcut kept.",
        [
            "{detail} Прежнее сочетание сохранено.",
            "{detail} Raccourci précédent conservé.",
            "{detail} Bisheriges Kürzel beibehalten.",
            "{detail} Se conserva el atajo anterior.",
            "{detail} Scorciatoia precedente mantenuta.",
        ],
    ),
    (
        "Correcting text...",
        [
            "Исправление текста…",
            "Correction du texte…",
            "Text wird korrigiert…",
            "Corrigiendo texto…",
            "Correzione del testo…",
        ],
    ),
    (
        "Text corrected. {shortcut} undoes the last fix.",
        [
            "Текст исправлен. {shortcut} отменяет исправление.",
            "Texte corrigé. {shortcut} annule la correction.",
            "Text korrigiert. {shortcut} macht dies rückgängig.",
            "Texto corregido. {shortcut} deshace la corrección.",
            "Testo corretto. {shortcut} annulla la correzione.",
        ],
    ),
    (
        "No update check yet.",
        [
            "Обновления ещё не проверялись.",
            "Aucune recherche de mise à jour.",
            "Noch nicht nach Updates gesucht.",
            "Aún no se buscaron actualizaciones.",
            "Aggiornamenti non ancora verificati.",
        ],
    ),
    (
        "Downloading and verifying update...",
        [
            "Загрузка и проверка обновления…",
            "Téléchargement et vérification…",
            "Update wird geladen und geprüft…",
            "Descargando y verificando…",
            "Download e verifica in corso…",
        ],
    ),
    (
        "Checking GitHub for updates...",
        [
            "Проверка обновлений на GitHub…",
            "Recherche sur GitHub…",
            "Updates auf GitHub werden gesucht…",
            "Buscando actualizaciones en GitHub…",
            "Ricerca aggiornamenti su GitHub…",
        ],
    ),
    (
        "Could not start the update: {detail}",
        [
            "Не удалось начать обновление: {detail}",
            "Impossible de lancer la mise à jour : {detail}",
            "Update konnte nicht starten: {detail}",
            "No se pudo iniciar la actualización: {detail}",
            "Impossibile avviare l’aggiornamento: {detail}",
        ],
    ),
    (
        "An update check or download is already in progress.",
        [
            "Проверка или загрузка уже выполняется.",
            "Une recherche ou un téléchargement est en cours.",
            "Suche oder Download läuft bereits.",
            "Ya hay una búsqueda o descarga en curso.",
            "Verifica o download già in corso.",
        ],
    ),
    (
        "Version {version} is available.",
        [
            "Доступна версия {version}.",
            "La version {version} est disponible.",
            "Version {version} ist verfügbar.",
            "Está disponible la versión {version}.",
            "È disponibile la versione {version}.",
        ],
    ),
    (
        "{status} Open About to update.",
        [
            "{status} Откройте «О программе».",
            "{status} Ouvrez À propos pour l’installer.",
            "{status} Zum Aktualisieren Info öffnen.",
            "{status} Abre Acerca de para actualizar.",
            "{status} Apri Informazioni per aggiornare.",
        ],
    ),
    (
        "Version {version} is available. Open About or the tray menu to download and install it.",
        [
            "Доступна версия {version}. Загрузите её в «О программе» или меню трея.",
            "Version {version} disponible. Installez-la via À propos ou le menu de la zone de notification.",
            "Version {version} verfügbar. Über Info oder das Infobereichmenü herunterladen und installieren.",
            "Versión {version} disponible. Descárgala e instálala desde Acerca de o el menú de la bandeja.",
            "Versione {version} disponibile. Scaricala da Informazioni o dal menu dell’area di notifica.",
        ],
    ),
    (
        "You are up to date.",
        [
            "Установлена последняя версия.",
            "L’application est à jour.",
            "Die App ist aktuell.",
            "La aplicación está actualizada.",
            "L’app è aggiornata.",
        ],
    ),
    (
        "You are up to date. Installed version: {version}.",
        [
            "Установлена последняя версия: {version}.",
            "L’application est à jour. Version installée : {version}.",
            "Die App ist aktuell. Installierte Version: {version}.",
            "La aplicación está actualizada. Versión instalada: {version}.",
            "L’app è aggiornata. Versione installata: {version}.",
        ],
    ),
    (
        "Update check or download failed: {detail}",
        [
            "Ошибка проверки или загрузки: {detail}",
            "Échec de la recherche ou du téléchargement : {detail}",
            "Suche oder Download fehlgeschlagen: {detail}",
            "Error al buscar o descargar: {detail}",
            "Verifica o download non riuscito: {detail}",
        ],
    ),
    (
        "Update failed. Your current app is unchanged.",
        [
            "Ошибка обновления. Текущая версия сохранена.",
            "Échec de la mise à jour. Votre application est intacte.",
            "Update fehlgeschlagen. Bisherige App unverändert.",
            "La actualización falló. Se conserva la aplicación actual.",
            "Aggiornamento non riuscito. L’app attuale è intatta.",
        ],
    ),
    (
        "Restarting to finish the update...",
        [
            "Перезапуск для завершения обновления…",
            "Redémarrage pour terminer…",
            "Neustart zum Abschließen des Updates…",
            "Reiniciando para terminar…",
            "Riavvio per completare…",
        ],
    ),
    (
        "Open Magic Language Switcher",
        [
            "Открыть Magic Language Switcher",
            "Ouvrir Magic Language Switcher",
            "Magic Language Switcher öffnen",
            "Abrir Magic Language Switcher",
            "Apri Magic Language Switcher",
        ],
    ),
    (
        "Pause corrections",
        [
            "Приостановить исправления",
            "Suspendre les corrections",
            "Korrekturen pausieren",
            "Pausar correcciones",
            "Sospendi correzioni",
        ],
    ),
    (
        "Resume corrections",
        [
            "Продолжить исправления",
            "Reprendre les corrections",
            "Korrekturen fortsetzen",
            "Reanudar correcciones",
            "Riprendi correzioni",
        ],
    ),
    (
        "Update in progress...",
        [
            "Идёт обновление…",
            "Mise à jour en cours…",
            "Update läuft…",
            "Actualizando…",
            "Aggiornamento in corso…",
        ],
    ),
    (
        "Download and install v{version}...",
        [
            "Загрузить и установить v{version}…",
            "Télécharger et installer v{version}…",
            "v{version} laden und installieren…",
            "Descargar e instalar v{version}…",
            "Scarica e installa v{version}…",
        ],
    ),
    (
        "Check for updates",
        [
            "Проверить обновления",
            "Rechercher des mises à jour",
            "Nach Updates suchen",
            "Buscar actualizaciones",
            "Verifica aggiornamenti",
        ],
    ),
    ("Quit", ["Выход", "Quitter", "Beenden", "Salir", "Esci"]),
    (
        "About Magic Language Switcher",
        [
            "О Magic Language Switcher",
            "À propos de Magic Language Switcher",
            "Über Magic Language Switcher",
            "Acerca de Magic Language Switcher",
            "Informazioni su Magic Language Switcher",
        ],
    ),
    (
        "Version {version}\n\nFix text typed in the wrong keyboard layout.\nWorks with the keyboard layouts installed in Windows.\n\n{status}\n\nBuilt with Rust. One portable EXE.",
        [
            "Версия {version}\n\nИсправляет текст, набранный в неверной раскладке.\nРаботает с установленными раскладками Windows.\n\n{status}\n\nСоздано на Rust. Один портативный EXE.",
            "Version {version}\n\nCorrige le texte saisi avec la mauvaise disposition.\nUtilise les dispositions installées dans Windows.\n\n{status}\n\nCréé avec Rust. Un seul EXE portable.",
            "Version {version}\n\nKorrigiert Text mit falschem Tastaturlayout.\nNutzt die in Windows installierten Layouts.\n\n{status}\n\nMit Rust erstellt. Eine portable EXE.",
            "Versión {version}\n\nCorrige texto escrito con la distribución incorrecta.\nUsa las distribuciones instaladas en Windows.\n\n{status}\n\nCreado con Rust. Un EXE portátil.",
            "Versione {version}\n\nCorregge il testo digitato con il layout errato.\nUsa i layout installati in Windows.\n\n{status}\n\nCreato con Rust. Un solo EXE portatile.",
        ],
    ),
    (
        "GitHub repository",
        [
            "Репозиторий GitHub",
            "Dépôt GitHub",
            "GitHub-Repository",
            "Repositorio de GitHub",
            "Repository GitHub",
        ],
    ),
    (
        "MIT license",
        [
            "Лицензия MIT",
            "Licence MIT",
            "MIT-Lizenz",
            "Licencia MIT",
            "Licenza MIT",
        ],
    ),
    (
        "Hide license",
        [
            "Скрыть лицензию",
            "Masquer la licence",
            "Lizenz ausblenden",
            "Ocultar licencia",
            "Nascondi licenza",
        ],
    ),
    (
        "Check for updates on startup",
        [
            "Проверять обновления при запуске",
            "Rechercher au démarrage",
            "Beim Start nach Updates suchen",
            "Buscar actualizaciones al iniciar",
            "Verifica aggiornamenti all’avvio",
        ],
    ),
    (
        "Magic Language Switcher update",
        [
            "Обновление Magic Language Switcher",
            "Mise à jour Magic Language Switcher",
            "Magic Language Switcher aktualisieren",
            "Actualizar Magic Language Switcher",
            "Aggiornamento Magic Language Switcher",
        ],
    ),
    (
        "Version {version} is available",
        [
            "Доступна версия {version}",
            "Version {version} disponible",
            "Version {version} verfügbar",
            "Versión {version} disponible",
            "Versione {version} disponibile",
        ],
    ),
    (
        "Installed: {version}\nDownload: {size} MB\n\nDownload from GitHub, verify SHA-256, and restart the app. Your settings and EXE location will be kept.",
        [
            "Установлено: {version}\nЗагрузка: {size} МБ\n\nЗагрузить с GitHub, проверить SHA-256 и перезапустить. Настройки и расположение EXE сохранятся.",
            "Version installée : {version}\nTéléchargement : {size} Mo\n\nTélécharger depuis GitHub, vérifier SHA-256 et redémarrer. Vos réglages et l’emplacement EXE seront conservés.",
            "Installiert: {version}\nDownload: {size} MB\n\nVon GitHub laden, SHA-256 prüfen und neu starten. Einstellungen und EXE-Speicherort bleiben erhalten.",
            "Instalada: {version}\nDescarga: {size} MB\n\nDescargar de GitHub, verificar SHA-256 y reiniciar. Se conservan los ajustes y la ubicación del EXE.",
            "Installata: {version}\nDownload: {size} MB\n\nScarica da GitHub, verifica SHA-256 e riavvia. Impostazioni e posizione EXE saranno mantenute.",
        ],
    ),
    (
        "Download and install",
        [
            "Загрузить и установить",
            "Télécharger et installer",
            "Laden und installieren",
            "Descargar e instalar",
            "Scarica e installa",
        ],
    ),
    (
        "Close",
        ["Закрыть", "Fermer", "Schließen", "Cerrar", "Chiudi"],
    ),
    (
        "Cancel",
        ["Отмена", "Annuler", "Abbrechen", "Cancelar", "Annulla"],
    ),
    (
        "Already in use: {shortcuts}",
        [
            "Уже занято: {shortcuts}",
            "Déjà utilisé : {shortcuts}",
            "Bereits belegt: {shortcuts}",
            "Ya en uso: {shortcuts}",
            "Già in uso: {shortcuts}",
        ],
    ),
    (
        "Already in use: {shortcuts}. Click the unavailable shortcut in settings to choose another combination.",
        [
            "Уже занято: {shortcuts}. Нажмите сочетание в настройках и выберите другое.",
            "Déjà utilisé : {shortcuts}. Cliquez sur le raccourci dans les réglages pour en choisir un autre.",
            "Bereits belegt: {shortcuts}. In den Einstellungen ein anderes Kürzel wählen.",
            "Ya en uso: {shortcuts}. Elige otra combinación en los ajustes.",
            "Già in uso: {shortcuts}. Scegli un’altra combinazione nelle impostazioni.",
        ],
    ),
    (
        "Could not save settings: {detail}",
        [
            "Не удалось сохранить настройки: {detail}",
            "Impossible d’enregistrer les réglages : {detail}",
            "Einstellungen konnten nicht gespeichert werden: {detail}",
            "No se pudieron guardar los ajustes: {detail}",
            "Impossibile salvare le impostazioni: {detail}",
        ],
    ),
    (
        "Text corrected, but {detail}",
        [
            "Текст исправлен, но {detail}",
            "Texte corrigé, mais {detail}",
            "Text korrigiert, aber {detail}",
            "Texto corregido, pero {detail}",
            "Testo corretto, ma {detail}",
        ],
    ),
    (
        "Release the shortcut keys and try again.",
        [
            "Отпустите клавиши и повторите.",
            "Relâchez les touches et réessayez.",
            "Tasten loslassen und erneut versuchen.",
            "Suelta las teclas e inténtalo de nuevo.",
            "Rilascia i tasti e riprova.",
        ],
    ),
    (
        "Password fields are not supported.",
        [
            "Поля пароля не поддерживаются.",
            "Les champs de mot de passe ne sont pas pris en charge.",
            "Passwortfelder werden nicht unterstützt.",
            "Los campos de contraseña no son compatibles.",
            "I campi password non sono supportati.",
        ],
    ),
    (
        "Could not copy the selected text. Nothing was changed.",
        [
            "Не удалось скопировать выделение. Ничего не изменено.",
            "Impossible de copier la sélection. Rien n’a été modifié.",
            "Auswahl konnte nicht kopiert werden. Nichts geändert.",
            "No se pudo copiar la selección. No se cambió nada.",
            "Impossibile copiare la selezione. Nessuna modifica.",
        ],
    ),
    (
        "There is no selected text to convert.",
        [
            "Нет выделенного текста для преобразования.",
            "Aucun texte sélectionné à convertir.",
            "Kein ausgewählter Text zum Umwandeln.",
            "No hay texto seleccionado para convertir.",
            "Nessun testo selezionato da convertire.",
        ],
    ),
    (
        "Select up to 4,096 characters at a time.",
        [
            "Выделите не более 4096 символов.",
            "Sélectionnez au plus 4 096 caractères.",
            "Höchstens 4.096 Zeichen auswählen.",
            "Selecciona hasta 4096 caracteres.",
            "Seleziona fino a 4096 caratteri.",
        ],
    ),
    (
        "This text has no characters that change between the chosen layouts.",
        [
            "В тексте нет символов, различающихся в выбранных раскладках.",
            "Aucun caractère ne change entre les dispositions choisies.",
            "Keine Zeichen ändern sich zwischen den gewählten Layouts.",
            "Ningún carácter cambia entre las distribuciones elegidas.",
            "Nessun carattere cambia tra i layout scelti.",
        ],
    ),
    (
        "Text corrected.",
        [
            "Текст исправлен.",
            "Texte corrigé.",
            "Text korrigiert.",
            "Texto corregido.",
            "Testo corretto.",
        ],
    ),
    (
        "Nothing to undo yet.",
        [
            "Пока нечего отменять.",
            "Rien à annuler pour le moment.",
            "Noch nichts rückgängig zu machen.",
            "Aún no hay nada que deshacer.",
            "Niente da annullare per ora.",
        ],
    ),
    (
        "Return to the same editor to undo the last fix.",
        [
            "Вернитесь в тот же редактор для отмены.",
            "Revenez au même éditeur pour annuler.",
            "Zum Rückgängigmachen zum selben Editor wechseln.",
            "Vuelve al mismo editor para deshacer.",
            "Torna allo stesso editor per annullare.",
        ],
    ),
    (
        "The text or caret moved. Undo directly after a correction.",
        [
            "Текст или курсор перемещён. Отменяйте сразу после исправления.",
            "Le texte ou le curseur a changé. Annulez juste après la correction.",
            "Text oder Cursor verschoben. Direkt nach der Korrektur rückgängig machen.",
            "El texto o el cursor cambió. Deshaz justo después de corregir.",
            "Testo o cursore spostato. Annulla subito dopo la correzione.",
        ],
    ),
    (
        "Last correction undone.",
        [
            "Последнее исправление отменено.",
            "Dernière correction annulée.",
            "Letzte Korrektur rückgängig gemacht.",
            "Última corrección deshecha.",
            "Ultima correzione annullata.",
        ],
    ),
    (
        "Add another keyboard layout in Windows Settings, then reopen this window.",
        [
            "Добавьте раскладку в параметрах Windows и откройте окно заново.",
            "Ajoutez une disposition dans Windows, puis rouvrez cette fenêtre.",
            "Weiteres Layout in Windows hinzufügen und Fenster neu öffnen.",
            "Añade otra distribución en Windows y vuelve a abrir esta ventana.",
            "Aggiungi un layout in Windows e riapri questa finestra.",
        ],
    ),
    (
        "The source layout is no longer installed. Reopen settings to refresh the list.",
        [
            "Исходная раскладка удалена. Откройте настройки заново.",
            "La disposition source a été supprimée. Rouvrez les réglages.",
            "Quelllayout nicht mehr installiert. Einstellungen neu öffnen.",
            "La distribución de origen ya no está instalada. Reabre los ajustes.",
            "Layout di origine non più installato. Riapri le impostazioni.",
        ],
    ),
    (
        "The destination layout is no longer installed. Reopen settings to refresh the list.",
        [
            "Конечная раскладка удалена. Откройте настройки заново.",
            "La disposition cible a été supprimée. Rouvrez les réglages.",
            "Ziellayout nicht mehr installiert. Einstellungen neu öffnen.",
            "La distribución de destino ya no está instalada. Reabre los ajustes.",
            "Layout di destinazione non più installato. Riapri le impostazioni.",
        ],
    ),
    (
        "Choose different From and To layouts.",
        [
            "Выберите разные исходную и конечную раскладки.",
            "Choisissez deux dispositions différentes.",
            "Unterschiedliche Von- und Nach-Layouts wählen.",
            "Elige distribuciones de origen y destino distintas.",
            "Scegli layout di origine e destinazione diversi.",
        ],
    ),
    (
        "Use Ctrl, Alt, or Shift. Windows-key shortcuts are not supported.",
        [
            "Используйте Ctrl, Alt или Shift. Клавиша Windows не поддерживается.",
            "Utilisez Ctrl, Alt ou Maj. La touche Windows n’est pas prise en charge.",
            "Ctrl, Alt oder Umschalt verwenden. Windows-Taste nicht unterstützt.",
            "Usa Ctrl, Alt o Mayús. La tecla Windows no es compatible.",
            "Usa Ctrl, Alt o Maiusc. Il tasto Windows non è supportato.",
        ],
    ),
    (
        "F12 is reserved by Windows. Choose another key.",
        [
            "F12 зарезервирована Windows. Выберите другую клавишу.",
            "F12 est réservé par Windows. Choisissez une autre touche.",
            "F12 ist für Windows reserviert. Andere Taste wählen.",
            "Windows reserva F12. Elige otra tecla.",
            "F12 è riservato a Windows. Scegli un altro tasto.",
        ],
    ),
    (
        "Choose a letter, number, function key, or navigation key.",
        [
            "Выберите букву, цифру, функциональную клавишу или клавишу навигации.",
            "Choisissez une lettre, un chiffre, une touche de fonction ou de navigation.",
            "Buchstaben, Zahl, Funktions- oder Navigationstaste wählen.",
            "Elige una letra, número o tecla de función o navegación.",
            "Scegli una lettera, numero o tasto funzione o navigazione.",
        ],
    ),
    (
        "Add Ctrl or Alt to this key, or choose a function key.",
        [
            "Добавьте Ctrl или Alt либо выберите функциональную клавишу.",
            "Ajoutez Ctrl ou Alt, ou choisissez une touche de fonction.",
            "Ctrl oder Alt hinzufügen oder Funktionstaste wählen.",
            "Añade Ctrl o Alt, o elige una tecla de función.",
            "Aggiungi Ctrl o Alt, oppure scegli un tasto funzione.",
        ],
    ),
    (
        "That shortcut is already assigned to another action.",
        [
            "Сочетание уже назначено другому действию.",
            "Ce raccourci est déjà affecté à une autre action.",
            "Kürzel bereits einer anderen Aktion zugewiesen.",
            "Ese atajo ya está asignado a otra acción.",
            "Scorciatoia già assegnata a un’altra azione.",
        ],
    ),
    (
        "Shortcut registration is unavailable.",
        [
            "Регистрация сочетаний недоступна.",
            "L’enregistrement des raccourcis est indisponible.",
            "Kürzelregistrierung nicht verfügbar.",
            "El registro de atajos no está disponible.",
            "Registrazione scorciatoie non disponibile.",
        ],
    ),
    (
        "Click inside an editor and try again.",
        [
            "Щёлкните в редакторе и повторите.",
            "Cliquez dans un éditeur et réessayez.",
            "In einen Editor klicken und erneut versuchen.",
            "Haz clic en un editor e inténtalo de nuevo.",
            "Fai clic in un editor e riprova.",
        ],
    ),
    (
        "The active editor changed. Try the shortcut again.",
        [
            "Активный редактор изменился. Нажмите сочетание ещё раз.",
            "L’éditeur actif a changé. Réessayez le raccourci.",
            "Aktiver Editor geändert. Kürzel erneut versuchen.",
            "El editor activo cambió. Prueba el atajo de nuevo.",
            "L’editor attivo è cambiato. Riprova la scorciatoia.",
        ],
    ),
    (
        "Windows could not send input. The editor may be running as administrator.",
        [
            "Windows не смогла отправить ввод. Возможно, редактор запущен администратором.",
            "Windows n’a pas pu envoyer la saisie. L’éditeur est peut-être lancé en administrateur.",
            "Windows konnte keine Eingabe senden. Editor läuft möglicherweise als Administrator.",
            "Windows no pudo enviar la entrada. Puede que el editor use permisos de administrador.",
            "Windows non ha potuto inviare l’input. L’editor potrebbe essere avviato come amministratore.",
        ],
    ),
    (
        "The clipboard is busy. Try again in a moment.",
        [
            "Буфер обмена занят. Повторите чуть позже.",
            "Le presse-papiers est occupé. Réessayez dans un instant.",
            "Zwischenablage belegt. Gleich erneut versuchen.",
            "El portapapeles está ocupado. Inténtalo en un momento.",
            "Appunti occupati. Riprova tra un momento.",
        ],
    ),
    (
        "The clipboard contains an app-specific format that cannot be preserved. Copy plain text first.",
        [
            "Буфер содержит формат, который нельзя сохранить. Сначала скопируйте обычный текст.",
            "Le presse-papiers contient un format non conservable. Copiez d’abord du texte brut.",
            "Zwischenablage enthält nicht erhaltbares App-Format. Zuerst Klartext kopieren.",
            "El portapapeles tiene un formato que no se puede conservar. Copia texto sin formato.",
            "Gli appunti contengono un formato non preservabile. Copia prima testo semplice.",
        ],
    ),
    (
        "Could not preserve the current clipboard. Try copying it again.",
        [
            "Не удалось сохранить буфер обмена. Скопируйте снова.",
            "Impossible de conserver le presse-papiers. Copiez à nouveau.",
            "Zwischenablage konnte nicht gesichert werden. Erneut kopieren.",
            "No se pudo conservar el portapapeles. Vuelve a copiar.",
            "Impossibile preservare gli appunti. Copia di nuovo.",
        ],
    ),
    (
        "Could not preserve the current clipboard format. Copy plain text first.",
        [
            "Не удалось сохранить формат буфера. Сначала скопируйте обычный текст.",
            "Impossible de conserver ce format. Copiez d’abord du texte brut.",
            "Zwischenablageformat konnte nicht gesichert werden. Zuerst Klartext kopieren.",
            "No se pudo conservar el formato. Copia texto sin formato.",
            "Impossibile preservare il formato. Copia prima testo semplice.",
        ],
    ),
    (
        "No text to correct. Place the caret after a word, or select text in an editor.",
        [
            "Нет текста. Поставьте курсор после слова или выделите текст.",
            "Aucun texte à corriger. Placez le curseur après un mot ou sélectionnez du texte.",
            "Kein Text. Cursor hinter ein Wort setzen oder Text auswählen.",
            "No hay texto. Coloca el cursor tras una palabra o selecciona texto.",
            "Nessun testo. Posiziona il cursore dopo una parola o seleziona testo.",
        ],
    ),
    (
        "Could not read the copied text. Nothing was changed.",
        [
            "Не удалось прочитать скопированный текст. Ничего не изменено.",
            "Impossible de lire le texte copié. Rien n’a été modifié.",
            "Kopierter Text nicht lesbar. Nichts geändert.",
            "No se pudo leer el texto copiado. No se cambió nada.",
            "Impossibile leggere il testo copiato. Nessuna modifica.",
        ],
    ),
    (
        "Windows could not restore the clipboard.",
        [
            "Windows не смогла восстановить буфер обмена.",
            "Windows n’a pas pu restaurer le presse-papiers.",
            "Windows konnte die Zwischenablage nicht wiederherstellen.",
            "Windows no pudo restaurar el portapapeles.",
            "Windows non ha potuto ripristinare gli appunti.",
        ],
    ),
    (
        "Some clipboard formats could not be restored.",
        [
            "Некоторые форматы буфера не удалось восстановить.",
            "Certains formats du presse-papiers n’ont pas été restaurés.",
            "Einige Zwischenablageformate konnten nicht wiederhergestellt werden.",
            "No se pudieron restaurar algunos formatos del portapapeles.",
            "Alcuni formati degli appunti non sono stati ripristinati.",
        ],
    ),
    (
        "The selection does not contain plain text.",
        [
            "Выделение не содержит обычного текста.",
            "La sélection ne contient pas de texte brut.",
            "Auswahl enthält keinen Klartext.",
            "La selección no contiene texto sin formato.",
            "La selezione non contiene testo semplice.",
        ],
    ),
    (
        "Could not read the selected text.",
        [
            "Не удалось прочитать выделенный текст.",
            "Impossible de lire le texte sélectionné.",
            "Ausgewählter Text konnte nicht gelesen werden.",
            "No se pudo leer el texto seleccionado.",
            "Impossibile leggere il testo selezionato.",
        ],
    ),
    (
        "Could not allocate clipboard memory.",
        [
            "Не удалось выделить память буфера.",
            "Impossible d’allouer la mémoire du presse-papiers.",
            "Speicher für Zwischenablage konnte nicht reserviert werden.",
            "No se pudo reservar memoria para el portapapeles.",
            "Impossibile allocare memoria per gli appunti.",
        ],
    ),
    (
        "Could not lock clipboard memory.",
        [
            "Не удалось заблокировать память буфера.",
            "Impossible de verrouiller la mémoire du presse-papiers.",
            "Zwischenablagespeicher konnte nicht gesperrt werden.",
            "No se pudo bloquear la memoria del portapapeles.",
            "Impossibile bloccare la memoria degli appunti.",
        ],
    ),
    (
        "Could not write the clipboard.",
        [
            "Не удалось записать в буфер обмена.",
            "Impossible d’écrire dans le presse-papiers.",
            "Zwischenablage konnte nicht beschrieben werden.",
            "No se pudo escribir en el portapapeles.",
            "Impossibile scrivere negli appunti.",
        ],
    ),
    (
        "Windows could not start shortcut recording.",
        [
            "Windows не смогла начать запись сочетания.",
            "Windows n’a pas pu enregistrer le raccourci.",
            "Windows konnte die Kürzelaufzeichnung nicht starten.",
            "Windows no pudo iniciar la grabación del atajo.",
            "Windows non ha potuto registrare la scorciatoia.",
        ],
    ),
    (
        "{shortcut} is unavailable or already used by another app.",
        [
            "{shortcut} недоступно или занято другим приложением.",
            "{shortcut} est indisponible ou utilisé par une autre application.",
            "{shortcut} ist nicht verfügbar oder durch eine andere App belegt.",
            "{shortcut} no está disponible o ya lo usa otra aplicación.",
            "{shortcut} non è disponibile o è usata da un’altra app.",
        ],
    ),
    (
        "Windows did not provide a local settings folder.",
        [
            "Windows не предоставила папку настроек.",
            "Windows n’a pas fourni de dossier de réglages.",
            "Windows hat keinen Einstellungsordner bereitgestellt.",
            "Windows no proporcionó una carpeta de ajustes.",
            "Windows non ha fornito una cartella per le impostazioni.",
        ],
    ),
    (
        "Invalid settings path.",
        [
            "Неверный путь настроек.",
            "Chemin des réglages invalide.",
            "Ungültiger Einstellungspfad.",
            "Ruta de ajustes no válida.",
            "Percorso impostazioni non valido.",
        ],
    ),
    (
        "Could not update Windows startup settings.",
        [
            "Не удалось изменить автозапуск Windows.",
            "Impossible de modifier le démarrage Windows.",
            "Windows-Autostart konnte nicht geändert werden.",
            "No se pudo cambiar el inicio con Windows.",
            "Impossibile modificare l’avvio con Windows.",
        ],
    ),
    (
        "The update worker stopped unexpectedly.",
        [
            "Обновление неожиданно остановилось.",
            "La mise à jour s’est arrêtée inopinément.",
            "Update unerwartet angehalten.",
            "La actualización se detuvo inesperadamente.",
            "L’aggiornamento si è interrotto inaspettatamente.",
        ],
    ),
    (
        "Connecting to GitHub...",
        [
            "Подключение к GitHub…",
            "Connexion à GitHub…",
            "Verbindung zu GitHub…",
            "Conectando con GitHub…",
            "Connessione a GitHub…",
        ],
    ),
    (
        "Downloading and verifying update: {percent}% ({received}/{total} MB)",
        [
            "Загрузка и проверка: {percent}% ({received}/{total} МБ)",
            "Téléchargement et vérification : {percent}% ({received}/{total} Mo)",
            "Download und Prüfung: {percent}% ({received}/{total} MB)",
            "Descarga y verificación: {percent}% ({received}/{total} MB)",
            "Download e verifica: {percent}% ({received}/{total} MB)",
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(value: &str) -> Vec<&str> {
        let mut keys: Vec<_> = value
            .split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(key, _)| key))
            .collect();
        keys.sort_unstable();
        keys
    }
    #[test]
    fn catalogs_have_unique_keys_complete_translations_and_matching_parameters() {
        let mut keys = std::collections::HashSet::new();
        for (key, translations) in MESSAGES {
            assert!(keys.insert(key), "duplicate message: {key}");
            for translation in translations {
                assert!(!translation.is_empty());
                assert_eq!(
                    placeholders(key),
                    placeholders(translation),
                    "{key}: {translation}"
                );
            }
        }
    }
    #[test]
    fn unknown_preferences_fall_back_and_editor_data_is_not_translated() {
        assert_eq!(Language::parse("unknown"), Language::English);
        assert_eq!(Theme::parse("unknown"), Theme::Light);
        for language in Language::ALL {
            assert_eq!(Language::parse(language.code()), language);
            assert_eq!(
                language.message("a user document.txt — Привет"),
                "a user document.txt — Привет"
            );
            let result =
                language.format("Version {version} is available.", &[("version", "12.3.4")]);
            assert!(result.contains("12.3.4"));
            assert!(!result.contains('{'));
        }
    }
    #[test]
    fn dynamic_values_are_inserted_verbatim_without_interpreting_their_braces() {
        for language in Language::ALL {
            let value = language.format(
                "{detail} Previous shortcut kept.",
                &[("detail", "user {version}.txt"), ("version", "1.2.3")],
            );
            assert!(value.contains("user {version}.txt"));
        }
    }
}
