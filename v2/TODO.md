# SLiM-CC v2 Status

Stand: 2026-07-17

Die im v1-Review dokumentierten kritischen Punkte wurden im v2-Neubau adressiert:

- FOMOD-Pfade bleiben innerhalb von Quelle und Modziel; Auswahlmodi, Required-Optionen und Prioritäten werden ausgewertet.
- FOMOD-Neukonfiguration ersetzt den vorhandenen Mod atomar und Vorschauen besitzen einen expliziten Lebenszyklus.
- Abhängigkeiten werden profilbezogen geprüft; manuelle Voraussetzungen können bestätigt werden.
- Direkter Real-Data-Deploy ist deaktiviert. Profile laufen über ein rootless `fuse-overlayfs`-VFS.
- Nexus-Dateidownload und Collection-Analyse verwenden die Nexus-APIs statt stiller Metadatenablage bzw. HTML-Scraping.
- Der Downloadordner hat einen sichtbaren Refresh und wird bei Fokus, Sichtbarkeit und im Intervall aktualisiert.
- Datei- und Ordnerimport sind ein gemeinsamer, kompakter Ablauf; technische Optionen liegen in Menüs und Dialogen.
- Data-Mods und Game-Root-Installer wie SKSE und der Engine-Fixes-Preloader werden getrennt in denselben virtuellen Profillayer eingeordnet.
- NMM-Links und signierte direkte `nexus-cdn.com`-Links besitzen produktive Live-Regressionstests.
- Frontend-, Rust-, VFS-Integrations- und Playwright-E2E-Tests sind über `scripts/test-all.sh` reproduzierbar.

## Vor einem öffentlichen Release

- Echten Nexus-Premium- und Nicht-Premium-Download jeweils mit einem aktuellen Link prüfen.
- Mindestens drei reale FOMOD-Pakete mit Required, SelectAny, SelectAtMostOne und Datei-Prioritäten installieren.
- Skyrim, SKSE, LOOT und xEdit jeweils aus demselben VFS-Profilpfad testen.
- RPM und AppImage auf einem zweiten Linux-System installieren und den `fuse-overlayfs`-Prerequisite-Check prüfen.
