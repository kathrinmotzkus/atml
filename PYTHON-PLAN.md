# Python-Plan für die ATML-Werkzeuge

## Zweck und Status

**Status: zurückgestellt.** Dieses Dokument sammelt technische Verbesserungen
für die Python-Werkzeuge, damit sie die weitere Arbeit an ATML nicht
unterbrechen. Die Punkte sind keine Voraussetzung für die aktuelle
Spezifikationsarbeit und werden später als eigene Arbeitseinheit umgesetzt.

Der gegenwärtige Schwerpunkt bleibt die ATML-Spezifikation und das fachlich
korrekte Konzeptdokument `examples/concept-vehicle-rental.atml`.

## Geprüfter Ist-Stand von `tools/build_atml.py`

Das Skript kann aus

- `grammar/toml-1.1.0.abnf` und
- `grammar/atml-ext.abnf`

eine syntaktisch gültige `grammar/atml.abnf` erzeugen. Die Prüfung wurde in
einer temporären Projektkopie durchgeführt, ohne die echte generierte Datei zu
verändern.

Erfolgreich geprüft wurden:

- der additive Build mit 107 TOML-Basisregeln,
- alle 91 vorhandenen Grammar-Tests,
- das vollständige Parsen von `concept-vehicle-rental.atml` mit der temporär
  erzeugten Grammatik,
- der Aufruf aus unterschiedlichen Arbeitsverzeichnissen,
- ein Projektpfad mit Leerzeichen,
- die Ausführung innerhalb und außerhalb eines venv,
- CRLF-Eingaben mit einheitlicher LF-Ausgabe.

Das Build-Skript selbst benötigt ausschließlich die Python-Standardbibliothek.
Ein venv und das externe Paket `abnf` sind erst für die nachgelagerten
Grammar-Tests erforderlich.

## Gesammelte Fallstricke

### Python-Kommando

Auf manchen Linux-Systemen ist nur `python3`, aber kein `python` vorhanden. In
einem aktivierten venv ist üblicherweise `python` verfügbar; unter Windows ist
häufig der Launcher `py -3` der zuverlässigste Aufruf.

Die spätere Dokumentation soll unterscheiden:

```sh
# Linux/macOS ohne venv
python3 tools/build_atml.py

# aktiviertes venv
python tools/build_atml.py

# Windows
py -3 tools\build_atml.py
```

### Python-Mindestversion

Der Parameter `newline` von `Path.write_text()` erfordert Python 3.10 oder
neuer. Diese Mindestversion ist noch nicht ausdrücklich dokumentiert. CI nutzt
Python 3.12; lokal wurde erfolgreich mit Python 3.13.5 geprüft.

### Ausführungsbit und Shebang

`tools/build_atml.py` besitzt eine Shebang, ist derzeit aber mit Dateimodus
`644` nicht direkt ausführbar. Deshalb scheitert `./tools/build_atml.py` auf
Unix-Systemen. Später ist zu entscheiden, ob ausschließlich der explizite
Interpreter-Aufruf dokumentiert oder zusätzlich das Ausführungsbit gesetzt
wird.

### Unterschied zwischen Build und Validierung

Die interne Prüfung des Build-Skripts kontrolliert nur den additiven Aufbau
anhand der Regelbezeichner und der Operatoren `=` beziehungsweise `=/`. Die
Erfolgsmeldung des Builds beweist noch nicht, dass die erzeugte ABNF syntaktisch
parsebar ist.

Nicht erkannt werden unter anderem:

- fehlerhafte ABNF-Ausdrücke,
- Referenzen auf unbekannte Regeln,
- doppelte Grunddefinitionen derselben `atml-*`-Regel,
- eine inkrementelle Definition ohne Grunddefinition,
- sonstige Probleme, die erst der ABNF-Parser erkennt.

Der Build und `tests/test_grammar.py` müssen deshalb als zusammengehöriger
Prüfablauf behandelt werden.

### Tests für enum-klassifizierte Vererbung

Das vollständige Konzeptdokument wird von der temporär erzeugten Grammatik
akzeptiert. Zusätzlich werden später gezielte positive und negative
Grammar-Tests benötigt.

Positive Beispiele:

```atml
[truck.light : vehicle.truck::light]
[child : catalog.vehicle.truck::light]
[child : vehicle.truck::light, defaults]
[[fleet : vehicle.truck::light, drive.petrol]]
```

Negative Beispiele:

```atml
[truck.light : truck::light]
[truck.light : vehicle.truck::]
[truck.light : vehicle::truck::light]
```

Die ABNF prüft dabei nur die syntaktische Form. Existenz und Typ der Tabelle,
Existenz des Enums und Mitgliedschaft des ausgewählten Werts bleiben
semantische Prüfungen.

### Verzeichnisstruktur und Pfade

Das Skript ermittelt das Projektverzeichnis robust aus `__file__` und ist damit
vom aktuellen Arbeitsverzeichnis unabhängig. Auch Leerzeichen und die
plattformabhängigen Pfadtrenner werden durch `pathlib.Path` korrekt behandelt.

Es setzt jedoch weiterhin diese feste Repository-Struktur voraus:

```text
project/
├── grammar/
│   ├── toml-1.1.0.abnf
│   ├── atml-ext.abnf
│   └── atml.abnf
└── tools/
    └── build_atml.py
```

Das alleinige Kopieren des Skripts an einen anderen Ort funktioniert deshalb
nicht. Durch `Path.resolve()` folgt ein symbolischer Link dem tatsächlichen
Ablageort des Skripts, was für einen Link auf die Repository-Datei sinnvoll
ist.

### Kodierung und Zeilenenden

Basis, Erweiterung und Ausgabe werden ausdrücklich als ASCII verarbeitet. Das
passt zur aktuellen ABNF, in der Unicode-Zeichen über `%x...` beschrieben
werden. Ein nicht-ASCII-Zeichen in einem Kommentar würde jedoch bereits einen
Kodierungsfehler auslösen.

CRLF-Eingaben werden von Python normalisiert; die Ausgabe wird deterministisch
mit LF geschrieben. Später ist zu entscheiden, ob ASCII als bewusste Regel
dokumentiert oder die Verarbeitung auf UTF-8 erweitert wird.

### Nicht atomare Ausgabedatei

`Path.write_text()` schreibt unmittelbar nach `grammar/atml.abnf`. Ein
Prozessabbruch während des Schreibens oder zwei gleichzeitige Builds könnten
eine unvollständige Datei hinterlassen. Eine spätere robuste Umsetzung soll
zunächst eine temporäre Datei im selben Verzeichnis vollständig schreiben und
sie anschließend atomar mit `Path.replace()` einsetzen.

### Fehlermeldungen

Fehlende Dateien, Schreibrechte oder Kodierungsprobleme erzeugen derzeit rohe
Python-Tracebacks. Später sollen Fehlermeldungen den betroffenen Pfad und die
Ursache knapp nennen und mit einem eindeutigen Fehlerstatus enden.

## Spätere Arbeitseinheit

Wenn die Python-Werkzeuge wieder in den Fokus kommen, ist folgende Reihenfolge
vorgesehen:

1. Python 3.10 als Mindestversion festlegen und dokumentieren.
2. Aufrufe für Linux, macOS, Windows und aktive venvs vereinheitlichen.
3. Entscheiden, ob Python-Skripte ein Ausführungsbit erhalten sollen.
4. Gezielte Grammar-Tests für enum-klassifizierte Vererbung ergänzen.
5. Build und ABNF-Validierung in einem dokumentierten Prüfkommando bündeln.
6. Die Ausgabedatei atomar erzeugen.
7. Verständliche Fehlerbehandlung für fehlende Dateien, Rechte und Kodierung
   ergänzen.
8. ASCII gegenüber UTF-8 bewusst entscheiden und testen.
9. Pfade mit Leerzeichen, CRLF, verschiedene Arbeitsverzeichnisse, venv und
   Windows-Aufrufe dauerhaft in Tests oder CI abdecken.

Bis zu dieser späteren Arbeitseinheit bleibt `tools/build_atml.py` funktional
unverändert.
