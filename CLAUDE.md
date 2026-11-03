# CLAUDE.md

Guide pour travailler sur ce dépôt (humains comme agents IA).

## 🥇 Règle d'or

**Le code doit être le plus professionnel possible et respecter les normes de
développement actuelles.** Concrètement, chaque changement :

- passe **tous** les contrôles sans warning : `cargo fmt`, `cargo clippy -D warnings`
  (lints du workspace dans `Cargo.toml`), `cargo test`, `npm run lint`, `npm run typecheck`,
  les tests WASM et e2e (voir [Commandes](#commandes)) ;
- est **testé** : un bug corrigé = un test qui échoue sans le correctif. Pas de test qui
  « saute » silencieusement : une fixture manquante est un échec ;
- garde l'API publique **documentée** (`missing_docs` est actif côté Rust, JSDoc côté TS) ;
- n'introduit ni `unwrap()` hors tests, ni `unsafe` non justifié, ni `any` / `!` en TypeScript ;
- est mesuré quand il touche la performance (chiffres avant / après dans le commit) ;
- respecte les conventions : **Conventional Commits** (`feat:`, `fix:`, `perf:`, `docs:`,
  `build:`, `ci:`, `chore:`…, `!` ou `BREAKING CHANGE:` pour un changement cassant),
  code et commentaires en anglais. Les messages de commit **sont** le changelog : ils
  doivent être clairs pour un utilisateur du SDK.

Si du code existant ne respecte pas cette règle, on le corrige au passage.

## 📌 Règle de documentation des cas particuliers

Tout comportement non évident — contournement d'un bug de dépendance, valeur
magique, ordre d'opérations imposé, limite d'une plateforme — **doit être expliqué
par un commentaire à l'endroit exact du code**, avec le *pourquoi* (idéalement la
mesure ou la source qui le justifie). Un cas non documenté est un bug en attente :
quelqu'un « nettoiera » le code et réintroduira le problème.

Les cas connus sont listés ci-dessous ; en ajouter un = le documenter dans le code
**et** dans cette liste.

## Architecture

```
crates/scanner-core/     Moteur Rust : stratégies de décodage, prétraitements, session temps réel
crates/scanner-wasm/     Bindings wasm-bindgen (API bas niveau, types TS générés via tsify)
packages/scanner-web/    SDK TypeScript publié sur npm (@gueshbzh-perso/scanner)
apps/demo-nuxt/          Démo Nuxt 4 (déployée sur GitHub Pages)
tests/fixtures/          Images de test versionnées (+ generate.py pour les régénérer)
```

Flux temps réel : caméra → `<video>` → WebGL2 (ROI, luma, réduction, 4 px/texel,
relecture asynchrone) → Web Worker → mémoire WASM → `ScanSession` (passe rapide +
1 stratégie de secours tournante, vérification 1D, confirmations, dé-duplication).

## Commandes

```bash
npm install                     # une fois, à la racine (npm workspaces)
npm run build                   # Rust → WASM (standard + SIMD) + TypeScript
npm run dev                     # démo sur http://localhost:3000
npm run tunnel                  # démo + tunnel Cloudflare (QR code) pour le téléphone
npm run lint && npm run typecheck
npm run test:e2e                # Chromium headless + fausse caméra (ffmpeg, Playwright)

cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p scanner-wasm --target wasm32-unknown-unknown --test web
```

`npm run build` installe seul la cible `wasm32-unknown-unknown` et la version de
`wasm-bindgen-cli` épinglée par `Cargo.lock` ; `wasm-opt` vient de `npm install` (binaryen). Hors de la CI, `CHROMIUM_PATH` permet
aux tests e2e d'utiliser un Chromium existant.

## Versions et publication

Une **seule version** pour tout le projet, jamais modifiée à la main :

1. Chaque push sur `main` → release-please (`.github/workflows/release.yml`) ouvre ou met
   à jour une **PR de release**. Elle calcule la version à partir des commits (`fix:` →
   patch, `feat:` → minor, `!` → major ; avant 1.0, un changement cassant fait un minor),
   met à jour `Cargo.toml` (`workspace.package.version`), `packages/scanner-web/package.json`,
   `CHANGELOG.md`, puis les deux lockfiles.
2. Fusionner cette PR → tag `vX.Y.Z`, release GitHub et **publication npm** du SDK
   (avec provenance).

Les crates Rust ne sont pas publiées (`publish = false`) : elles sont livrées dans le
paquet npm. Lancer manuellement le workflow *Release* (`workflow_dispatch`) publie la
version courante (utilisé pour la première publication).

## Cas particuliers connus

| Cas | Où c'est géré / documenté |
| --- | --- |
| rxing horodate chaque résultat avec `chrono::Utc::now()` : sans la feature `wasmbind`, panique « time not implemented » dans le navigateur | `crates/scanner-wasm/Cargo.toml`, test `tests/web.rs` |
| `PossibleFormats` de rxing pas strictement respecté → filtre après décodage ; liste toujours explicite, sinon rxing lit aussi RSS/Telepen… (`Unknown`) | `decode/rxing_backend.rs` |
| Échecs checksum / format de rxing = « rien trouvé », pas une erreur | `decode/rxing_backend.rs` (`map_error`) |
| Faux positifs 1D (surtout UPC-E) sur des textures, **stables d'une frame à l'autre** → vérification de la zone de silence | `decode/verify.rs`, test `tests/linear_formats.rs` |
| Points rxing inutilisables pour Code 39 (inversés, à l'intérieur) et Codabar (fin = bout de ligne) → exemptés de la vérification | `decode/verify.rs` (`geometry`) |
| Position des points 1D propre à chaque symbologie (milieu des motifs start/stop) | `decode/verify.rs` (`geometry`) |
| Codes 1D confirmés sur plusieurs frames (2, formats fragiles 3), 2D sur 1 frame | `result.rs` (`required_confirmations`), `realtime/tracker.rs` |
| Micro QR lu par le lecteur QR de rxing et rapporté comme `qr_code` | `result.rs` |
| Rotation des stratégies : une seule stratégie coûteuse par frame (×6,6 sur les frames vides) | `realtime/session.rs` |
| Throttle adaptatif désactivé par le SDK (le worker gère déjà la contre-pression) | `packages/scanner-web/src/core.ts` |
| Vue `frameBuffer()` invalidée à chaque croissance de la mémoire WASM : ne jamais la garder | `crates/scanner-wasm/src/lib.rs` |
| Une seule frame en vol : on saute des frames au lieu de les empiler | `src/reader.ts` |
| Trap WASM (panique Rust) = module dans un état indéfini → le worker est recréé | `src/worker.ts`, `src/engine.ts` |
| Seules les erreurs du pipeline d'image (`GrabError`) déclenchent le repli WebGL → Canvas 2D | `src/reader.ts` |
| Ligne 0 du framebuffer WebGL = haut de l'image (pas de flip) ; largeur de sortie multiple de 4 | `src/grabber.ts`, `src/geometry.ts` |
| Les objets `URL` ne passent pas `postMessage` → on ne transmet que les options moteur | `src/core.ts` (`pickEngineOptions`) |
| Glue JS identique pour les builds WASM standard et SIMD (vérifié au build) | `scripts/build-wasm.mjs` |
| Son de détection : l'`AudioContext` doit être créé pendant un geste utilisateur (iOS) | `src/feedback.ts`, `src/reader.ts` |
| Vite/Nuxt : `optimizeDeps.exclude` obligatoire, sinon les URL du worker et du `.wasm` cassent | `apps/demo-nuxt/nuxt.config.ts`, README du SDK |
| Tunnel Cloudflare : hôte `*.trycloudflare.com` autorisé ; QR code seulement si stdout est un terminal | `apps/demo-nuxt/nuxt.config.ts`, README de la démo |
| Démo sur le domaine `scanner.romrd.fr` (racine, donc pas de `NUXT_APP_BASE_URL`). Le domaine se règle dans *Settings → Pages* ; le fichier `CNAME` est ignoré pour un déploiement Actions mais gardé par sécurité | `.github/workflows/pages.yml`, `apps/demo-nuxt/public/CNAME` |
| `allow-unwrap-in-tests` de clippy ne couvre pas `#[wasm_bindgen_test]` | `crates/scanner-wasm/tests/web.rs` |
| release-please met à jour `Cargo.toml` et `package.json` mais pas les lockfiles (qui recopient la version) → job dédié sur la PR de release | `.github/workflows/release.yml` (`sync-lockfiles`) |
| Publication npm par OIDC (trusted publishing) : npm ≥ 11.5.1 requis, Node 22 n'embarque que npm 10 | `.github/workflows/release.yml` |
| `bootstrap-sha` de release-please = dernier commit de la 0.4.0 : sans lui, la première exécution (aucune release existante) reprendrait tout l'historique. Ne sert plus dès que le tag `v0.4.0` existe ; impose une fusion par *merge commit* (pas *squash*) | `release-please-config.json` |
| `wasm-opt` vient de la devDependency `binaryen` (dans le PATH via `npm run`) ; sans lui le build marche mais le `.wasm` est ~7 % plus gros | `packages/scanner-web/scripts/build-wasm.mjs` |

## Tests et fixtures

- `tests/fixtures/` est **versionné** : les tests ne dépendent pas de Python. On ne
  relance `generate.py` que pour ajouter ou modifier une image, puis on met à jour
  les valeurs attendues.
- Les cas les plus durs sont **synthétisés dans les tests** (encodeurs rxing, flou,
  bruit, textures) : c'est déterministe et ça couvre chaque format.
- Ce qui manque encore : un jeu de **vraies photos** (reflets, codes abîmés, angles)
  pour un benchmark réaliste. Toute photo ajoutée doit avoir sa valeur attendue.
