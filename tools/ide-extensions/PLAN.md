# Entwicklungsplan für die ATML-IDE-Erweiterungen

## Ziel

Das erste Produkt ist eine VS-Code-Erweiterung für `.atml`-Dateien. Sie soll
ATML-Dokumente zuverlässig hervorheben, während der Eingabe prüfen, verstehen
und bei der Bearbeitung unterstützen. Die Sprachlogik bleibt im
editorunabhängigen Rust-Kern, damit derselbe Language Server später auch mit
anderen LSP-fähigen Editoren verwendet werden kann.

Die Entwicklung erfolgt in kleinen, jeweils testbaren Meilensteinen. Ein
Meilenstein gilt erst als abgeschlossen, wenn Implementierung, automatisierte
Tests und Benutzerdokumentation gemeinsam fertig sind.

## Aktueller Stand

Das Grundgerüst der Entwicklungsstufe 1 ist vorhanden:

- Rust-Workspace mit `atml-language-core` und `atml-language-server`
- `toml_dom 0.4.0` als Parser- und Dokumentmodell
- LSP-Kommunikation über stdin/stdout
- inkrementelle Textsynchronisation
- grundlegende Syntaxdiagnosen und Dokument-Symbole
- VS-Code-Client in TypeScript
- Sprachkonfiguration und erstes TextMate-Highlighting
- lokale Entwicklungsumgebung über F5 und Cargo
- Rust- und TypeScript-Builds sowie erste Unit-Tests

## Meilenstein 1: Grundgerüst stabilisieren

**Status: abgeschlossen.** Der Meilenstein wird durch Rust-Unit- und
Prozesstests, TextMate-Token-Tests, einen echten VS-Code-Smoke-Test und die
GitHub-Actions-CI abgesichert.

### Aufgaben

1. Einen echten LSP-Integrationstest ergänzen, der den Server als Prozess
   startet und den Ablauf `initialize` → `didOpen` → Diagnose → `shutdown`
   prüft.
2. Tests für inkrementelle Änderungen mit ASCII, Umlauten und Zeichen außerhalb
   der Unicode-BMP ergänzen.
3. Änderungen kurz verzögern und zusammenfassen, damit nicht bei jedem einzelnen
   Tastendruck eine veraltete Dokumentversion analysiert wird.
4. Sicherstellen, dass Ergebnisse älterer Dokumentversionen niemals an den
   Client gesendet werden.
5. Server-Logging über das VS-Code-Ausgabefenster ergänzen, ohne das
   LSP-Protokoll auf stdout zu beschädigen.
6. Dokument-Symbole hierarchisch darstellen: Tabellen enthalten ihre Schlüssel
   und Untertabellen.
7. Das TextMate-Highlighting anhand aller Dateien in `examples/` und eines
   gezielten Syntax-Fixtures visuell und automatisiert prüfen.
8. Einen VS-Code-Smoke-Test für Aktivierung, Dateizuordnung und Serverstart
   anlegen.

### Abnahmekriterien

- Eine gültige Beispieldatei erzeugt keine Diagnose.
- Ein eingefügter Syntaxfehler erscheint ohne Serverneustart und verschwindet
  nach seiner Korrektur wieder.
- Unicode-Positionen markieren exakt das fehlerhafte Zeichen.
- Die Outline zeigt Tabellen und Schlüssel in nachvollziehbarer Hierarchie.
- Rust-Tests, Clippy, TypeScript-Prüfung und VS-Code-Smoke-Test laufen in CI.

## Meilenstein 2: Semantisches ATML-Modell

**Status: abgeschlossen.** `atml-language-core` erzeugt pro gültigem
Dokumentsnapshot einen editorneutralen semantischen Index mit stabilen
Definition-IDs, Typen und UTF-8-Quellbereichen. Der Server hält diesen Index
zusammen mit der Syntaxanalyse im Cache der jeweiligen Dokumentversion.

### Aufgaben

1. Aus dem `toml_dom`-Dokument einen editorneutralen Symbolindex aufbauen:
   - Schlüssel und Tabellen
   - Enum-Definitionen und Enum-Mitglieder
   - Enum-Referenzen
   - Bare Path References
   - Elternbeziehungen geerbter Tabellen und Arrays-of-Tables
   - Quantities und ihre Einheiten
2. Für jedes Symbol Definition, Quellbereich, Sichtbarkeit und Typinformation
   speichern.
3. Referenzen transitiv auflösen, ohne die geschriebene Dokumentstruktur zu
   verlieren.
4. Tabellenvererbung als gerichteten Graph modellieren und effektive Tabellen
   über `toml_dom` berechnen.
5. Teilergebnisse pro unveränderter Dokumentversion zwischenspeichern.

### Abnahmekriterien

- Der Index bildet alle ATML-Konstrukte der aktuellen `atml.abnf` ab.
- Definitionen und Verwendungen lassen sich eindeutig miteinander verbinden.
- Zyklen werden erkannt, ohne Rekursion oder Serverprozess zum Absturz zu
  bringen.
- Tests verwenden sowohl kleine isolierte Fälle als auch `vehicle-rental.atml`
  und die übrigen offiziellen Beispiele.

## Meilenstein 3: Semantische Diagnosen

**Status: abgeschlossen.** Syntax-, TOML- und ATML-Semantikfehler besitzen
stabile Codes, Schweregrade und Originalquellbereiche. Eine positionsstabile
Wiederherstellung ermöglicht mehrere unabhängige Referenzdiagnosen in einem
Dokument, obwohl `toml_dom` unbekannte und zyklische Pfade bereits beim Parsen
abweist.

### Aufgaben

Diagnosen mit stabilen Codes und präzisen Quellbereichen implementieren:

- unbekanntes Ziel einer Bare Path Reference
- zyklische Pfadreferenz
- unbekannte Enum-Definition
- unbekanntes Enum-Mitglied
- Enum-Verwendung vor ihrer Definition
- unbekannte Elterntabelle
- ungültiger Eltern-Typ
- zyklische Tabellenvererbung
- semantisch ungültige Array-of-Tables-Vererbung

Syntaxfehler und semantische Fehler werden getrennt kategorisiert. Eine
fehlerhafte Stelle darf nach Möglichkeit nicht verhindern, dass unabhängige
Bereiche desselben Dokuments analysiert werden.

### Abnahmekriterien

- Jede Diagnose besitzt einen dokumentierten, stabilen Code.
- Tests prüfen Code, Schweregrad und Quellbereich, nicht nur den Meldungstext.
- Keine Diagnose wird für gültige offizielle Beispiele ausgegeben.
- Mehrere voneinander unabhängige Fehler können gleichzeitig angezeigt werden.

## Meilenstein 4: Completion

**Status: abgeschlossen.**

### Reihenfolge

1. Enum-Mitglieder nach `Enum::`
2. Schlüsselpfade bei Bare Path References
3. Elterntabellen nach `:` in Tabellenköpfen
4. sichtbare Enum-Namen an Wertpositionen
5. TOML- und ATML-Strukturelemente an leeren Wertpositionen
6. bekannte Einheiten bei Quantities

### Anforderungen

- Vorschläge berücksichtigen Cursorposition, Gültigkeitsbereich und bereits
  eingegebenes Präfix.
- Jeder Eintrag enthält Art, Detailinformation und den richtigen zu ersetzenden
  Textbereich.
- Completion funktioniert auch in einem vorübergehend syntaktisch unvollständigen
  Dokument, etwa direkt nach `Strategy::` oder `[child :`.
- Sortierung bevorzugt lokal definierte und typkompatible Symbole.

### Abnahmekriterien

- Completion-Tests decken jeden Kontext sowie negative Kontexte ab.
- Vorschläge erscheinen nach einer normalen Bearbeitung ohne wahrnehmbare
  Verzögerung.
- Der Server schlägt keine Symbole aus einem ungültigen Gültigkeitsbereich vor.

## Meilenstein 5: Hover und Navigation

**Status: abgeschlossen.**

### Aufgaben

1. Hover für Schlüssel mit Werttyp und Definition.
2. Hover für Quantities mit Magnitude, Unit, Exponent und Super-Unit.
3. Hover für Enum-Referenzen mit Definition und zulässigen Mitgliedern.
4. Hover für Pfadreferenzen mit Zielpfad und aufgelöstem Wert.
5. Hover für geerbte Werte mit Angabe der ursprünglichen Elterntabelle.
6. Go-to-Definition für Enum-Referenzen, Pfadreferenzen und Elterntabellen.
7. Find References für Enum-Definitionen, Schlüssel und Tabellen.

### Abnahmekriterien

- Alle Navigationsziele zeigen exakt auf den definierenden Namen.
- Hover-Ausgaben sind kompakt und als Markdown sicher darstellbar.
- Navigation bleibt auch bei transitiven Referenzen nachvollziehbar.

## Meilenstein 6: Robustes Bearbeiten

**Status: abgeschlossen.**

### Aufgaben

1. Tolerante Analyse für während der Eingabe unvollständige Dokumente.
2. Semantic Tokens ergänzen, wenn TextMate-Kontext nicht ausreicht.
3. Sichere Umbenennung von Schlüsseln, Enums und Tabellen.
4. Code Actions für eindeutig korrigierbare Fehler.
5. Erst danach ein Formatierungskonzept auf Grundlage der
   format-erhaltenden `toml_dom`-Operationen spezifizieren.

Formatierung wird nicht als einfacher Pretty Printer umgesetzt: Kommentare,
Schreibweisen und unveränderte Bereiche müssen gemäß dem DST/CST-Modell von
`toml_dom` erhalten bleiben.

## Meilenstein 7: Veröffentlichung der VS-Code-Erweiterung

**Status: technisch vorbereitet; externe Veröffentlichung ausstehend.**

### Aufgaben

1. Name, Publisher-ID, Icon, Lizenztexte und Marketplace-Texte finalisieren.
2. Native Server-Binaries für Linux, Windows und macOS erzeugen.
3. Architekturvarianten mindestens für x86_64 und ARM64 festlegen.
4. Binaries automatisiert in das VSIX-Paket übernehmen.
5. Prüfsummen und reproduzierbare Release-Artefakte erzeugen.
6. CI-Matrix für Rust, TypeScript und Extension-Tests einrichten.
7. Installations-, Konfigurations- und Fehlerbehebungsdokumentation schreiben.
8. Eine Vorabversion intern testen, anschließend Version `0.1.0`
   veröffentlichen.

### Abnahmekriterien

- Die Installation benötigt weder Rust noch Node.js auf dem Zielsystem.
- Die Erweiterung startet auf allen unterstützten Plattformen denselben
  getesteten Language Server.
- Das VSIX enthält keine Quellen, Caches, Tokens oder unnötigen Abhängigkeiten.
- Eine frische Installation erkennt `.atml` unmittelbar und zeigt Diagnosen,
  Completion, Hover und Navigation.

## Querschnittsaufgaben

**Status: umgesetzt und durch CI-Prüfungen abgesichert.** Der Rust-Kern wird
zusätzlich mit einem deterministischen Korpus beliebiger Unicode-Skalarwerte
und einem synthetischen ATML-Dokument mit 10.000 Zeilen geprüft. Die
VS-Code-Manifesttexte liegen in synchronisierten englischen und deutschen
Lokalisierungsdateien. Zwei coverage-gesteuerte Fuzz-Targets prüfen Analyse
und editorseitige Sprachfunktionen in kurzen CI-Läufen und längeren lokalen
Kampagnen.

Diese Arbeiten begleiten jeden Meilenstein:

- Änderungen an `grammar/atml.abnf` in neue Parser-, Diagnose- und
  Highlighting-Tests übersetzen.
- Die verwendete Mindestversion von `toml_dom` explizit halten und Upgrades
  gegen den gesamten Fixture-Bestand prüfen.
- Performance großer Dokumente messen; zunächst gelten 10.000 Zeilen als
  realistischer Belastungstest.
- Abstürze und Panics bei beliebigen UTF-8-Eingaben ausschließen.
- LSP-Funktionen im Rust-Kern testen; VS Code bleibt ein möglichst dünner
  Adapter.
- Benutzertexte auf Englisch implementieren und eine spätere deutsche
  Lokalisierung vorbereiten.

## Umsetzungsstand und nächste Arbeitseinheit

Die technische Vorbereitung von Meilenstein 7 ist abgeschlossen. Marketplace-
Texte, Icon, Lizenz und Changelog sind finalisiert. Eine CI-Matrix erzeugt sechs
zielplattform-spezifische VSIX-Pakete für Linux, Windows und macOS auf x64 und
ARM64. Jedes Paket enthält ein gebündeltes JavaScript und genau ein natives
Server-Binary, wird auf unerwünschte Inhalte geprüft, deterministisch
normalisiert und mit SHA-256 versehen. Das Linux-x64-Paket wurde reproduzierbar
gebaut, im vollständigen Extension-Test geprüft und in ein isoliertes VS-Code-
Profil installiert.

Die externe Veröffentlichung von Version `0.1.0` bleibt der geschützte letzte
Release-Schritt. Sie setzt die Bestätigung des Marketplace-Publishers
`kathrinmotzkus`, ein kurzlebiges `VSCE_PAT` außerhalb des Repositories, die
sechs durch CI erzeugten Artefakte sowie die bewusste Freigabe zum Publizieren
voraus. Der genaue Ablauf steht in `RELEASING.md`.

## Update-Plan nach Version 0.1.2

**Status: Analyse begonnen; Entscheidungen und Umsetzung ausstehend.** Weitere
Beobachtungen werden zunächst gesammelt, bevor Spezifikation, `toml_dom` oder
Language Server geändert werden. Die folgenden Punkte beschreiben daher noch
keine beschlossene ATML-Semantik.

### Beobachtung: Nachfahrentabellen erscheinen als geerbte Werte

Beim Hover über einen Vererbungspfad wie `class.truck.light` erscheinen auch
Werte aus den Geschwistertabellen `class.truck.medium` und
`class.truck.heavy`. Das Problem betrifft mindestens drei getrennt zu
untersuchende Ebenen:

1. **ATML-Spezifikation:** `atml.abnf` erlaubt geerbte Tabellen mit Dotted
   Keys, kann aber als kontextfreie Grammatik nicht festlegen, ob explizit
   deklarierte Nachfahrentabellen zu den vererbbaren Werten eines Elternteils
   gehören. `SPEC.md` spricht von vollständig aufgelösten `key/values`, grenzt
   benannte Nachfahrentabellen bislang jedoch nicht eindeutig ab.
2. **Dokumentmodell:** `vehicle-rental.atml` ist syntaktisch gültig. Die nicht
   ausdrücklich deklarierte Tabelle `class` entsteht nach den TOML-Regeln
   implizit; eine zusätzliche `[class]`-Deklaration würde das Problem nicht
   lösen. Die hierarchische Anordnung der Vorlagen macht aber eine bislang
   unklare Vererbungsfrage sichtbar.
3. **Implementierungen:** Die Hover-Auswertung sammelt Schlüssel derzeit über
   ein reines Pfadpräfix und ordnet dadurch Nachfahrentabellen dem Elternteil
   zu. `toml_dom` führt eine flache Tabellenzusammenführung durch, bei der
   Tabellenwerte ebenfalls übernommen werden können. Deshalb darf die
   Korrektur nicht ungeprüft auf die sichtbare Hover-Ausgabe begrenzt werden.

Für `class.truck.light` wird als erwartetes Ergebnis zur Diskussion gestellt:

- unmittelbar aus `class.truck.light`: `licence_class`, `cargo_volume`,
  `deposit`, `base_rate`
- transitiv aus `class.truck`: `wheels`, `seats`
- transitiv aus `vehicle`: `currency`, `min_rental_period`, `status`
- nicht enthalten: Werte oder Tabellen aus `class.truck.medium` und
  `class.truck.heavy`

### Vorgeschlagene Entscheidungs- und Arbeitsschritte

1. Alle weiteren beobachteten Fälle sammeln und in kleine, erwartungsbasierte
   Beispiele zerlegen.
2. Entscheiden und in `SPEC.md` normativ festhalten, ob durch eigene Tabellen-
   oder Array-of-Tables-Header deklarierte Nachfahren von der Vererbung
   ausgeschlossen werden. Dabei Dotted Keys, Inline Tables, implizite Tabellen
   und explizite Untertabellen getrennt betrachten.
3. Aus der Entscheidung gemeinsame Konformitätstests für ATML,
   `vehicle-rental.atml`, `toml_dom` und den Language Server ableiten.
4. Prüfen, welche effektiven Tabellen `toml_dom` derzeit für die betroffenen
   Standardtabellen und `fleet`-Elemente erzeugt. Eine reine Hover-Korrektur ist
   nur zulässig, wenn das zugrunde liegende Dokumentmodell bereits korrekt ist.
5. Falls erforderlich, `toml_dom` um die Herkunft beziehungsweise
   Deklarationseigentümerschaft von Tabellenwerten ergänzen, die Auflösung
   korrigieren, eine Patch-Version veröffentlichen und erst danach die
   IDE-Abhängigkeit aktualisieren.
6. Im semantischen IDE-Index jeden Schlüssel der konkreten Tabellen- oder
   Array-Element-Deklaration zuordnen. Präfixvergleiche allein dürfen nicht
   bestimmen, welche Werte zu einem Elternteil gehören.
7. Hover, Completion, Navigation, Diagnosen, Semantic Tokens und Rename auf
   dieselbe Eigentums- und Vererbungslogik prüfen, damit keine Funktion ein
   abweichendes Tabellenmodell verwendet.
8. Regressionstests mindestens für `class.truck.light`,
   `class.truck.medium`, `class.truck.heavy`, mehrere `[[fleet]]`-Elemente und
   verschachtelte Tabellen ergänzen.
9. Erst nach vollständigen Kern-, LSP-, VS-Code-, Grammar- und Fuzz-Tests eine
   neue Patch-Version der Erweiterung vorbereiten.

### Nicht empfohlene Abkürzungen

- Das Beispieldokument nur durch flache Namen wie `truck_light` umzuschreiben,
  würde die ungeklärte Semantik verdecken.
- Nur die Hover-Liste zu filtern, könnte eine korrekte Anzeige über einem
  weiterhin falschen effektiven Dokumentmodell erzeugen.
- Eine ausdrückliche `[class]`-Deklaration ändert weder die Geschwisterstruktur
  noch die Vererbungsauflösung.
