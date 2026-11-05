# CLAUDE.md

Guide pour travailler sur ce dépôt (humains comme agents IA).

## 🥇 Règle d'or

**Le code doit être le plus professionnel possible et respecter les normes de
développement actuelles.** Concrètement, chaque changement :

- passe **tous** les contrôles sans warning : `cargo fmt`, `cargo clippy -D warnings`
  (lints du workspace dans `Cargo.toml`), `cargo test`, `cargo doc` (`-D warnings`), `npm run lint`, `npm run typecheck`,
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
relecture asynchrone) → pool de Web Workers en parallèle (`FrameDecoder` : passe rapide +
1 stratégie de secours, vérification 1D) ⇄ `ScanSession` sur le thread principal
(`plan` : choix de la stratégie, partage du temps CPU ; `commit` : confirmations, dé-duplication).
En option, le `BarcodeDetector` natif du navigateur fait la passe rapide sur le `<video>` (`commit_external`),
et les workers ne lancent plus qu'un secours de temps en temps quand il ne trouve rien.

## Commandes

```bash
npm install                     # une fois, à la racine (npm workspaces)
npm run build                   # Rust → WASM + TypeScript
npm run dev                     # démo sur http://localhost:3000
npm run tunnel                  # démo + tunnel Cloudflare (QR code) pour le téléphone
npm run lint && npm run typecheck
npm test                        # tests unitaires du SDK (node:test, sur dist/ : après le build)
npm run test:e2e                # Chromium headless + fausse caméra (ffmpeg, Playwright)

cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo audit --deny warnings      # vulnérabilités / crates non maintenues (comme la CI)
cargo test --workspace --release
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace   # liens de doc (comme la CI)
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
   (avec provenance si le dépôt est public : npm la refuse pour un dépôt privé).
   Avant de fusionner, vérifier le job `release-pr-ci` du workflow *Release* : c'est la
   seule CI de la PR de release (voir le tableau ci-dessous).

**Fusion d'une PR : toujours un *merge commit* dont le message n'est pas un commit
conventionnel** (ex. titre `Merge pull request #N …`, corps `PR #N`). GitHub met par
défaut le titre de la PR dans le corps du merge commit ; s'il commence par `feat:` ou
`fix:`, release-please le compte comme un commit de plus et la ligne apparaît en double
dans le changelog (c'est arrivé pour la 0.5.0).

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
| Surlignage des codes 1D = **boîte du symbole** (4 coins), pas les 2 points de la ligne de lecture : la ligne change d'une frame à l'autre (la barre « montait et descendait ») et les points rxing sont faux pour Code 39 (fin calculée `(lastStart + lastPatternSize) / 2` au lieu de `lastStart + lastPatternSize / 2` : ~15 % du code) et Codabar (fin = bout de ligne). Extrémités : géométrie de la symbologie quand les points sont fiables, avec un module **mesuré sur les barres** (le déduire du texte suppose un caractère par symbole, faux en Code 128 jeu C), sinon mesurées sur l'image jusqu'à la zone de silence ; hauteur : chaque pixel de barre de la ligne lue est **suivi** rangée par rangée (±1 px pour l'inclinaison, 2 rangées claires tolérées), arrêt quand moins de 60 % continuent. Comparer chaque rangée à la précédente dérivait dans les chiffres de la boucle (boîte = toute l'image), la comparer à la seule ligne lue s'arrêtait à mi-hauteur sur l'étiquette. Ne pas corriger les formules de rxing : il faudrait défaire ses transformations (ligne inversée, rotation à 90°, recadrages) | `decode/extent.rs`, `decode/verify.rs` (`geometry`), tests `linear_codes_points_are_the_symbol_box`, `feed_label_code39_points_span_the_symbol`, `ear_tag_box_hugs_the_bars` |
| Position des points 1D propre à chaque symbologie (milieu des motifs start/stop) | `decode/verify.rs` (`geometry`) |
| Codes 1D confirmés sur plusieurs frames (2, formats fragiles 3), 2D sur 1 frame | `result.rs` (`required_confirmations`), `realtime/tracker.rs` |
| Zone de silence exigée : 5 modules pour les formats fragiles, **2** pour les formats à checksum (Code 128, EAN-13, UPC-A) — les vraies étiquettes/boucles ont souvent bien moins que la norme (bord de la boucle à ~3 modules sur la fixture) | `decode/verify.rs` (`STRONG_QUIET_MODULES`) |
| Gravure laser / éclairage inégal : stratégie `Linear` = lissage dans le sens des barres + seuillage local par ligne. Sortie **binaire** obligatoire (une sortie en niveaux de gris est re-seuillée par `HybridBinarizer` et le code est perdu). Deux échelles de fenêtre (`Fine`, `Coarse`) car la fenêtre idéale ≈ 6 modules et la taille des modules est inconnue | `preprocess/linear.rs`, fixture `real/cattle_ear_tag_code128.jpg` |
| La vérification de zone de silence de `Linear` se fait sur l'image traitée (sur l'image brute, le dégradé d'éclairage ressemble à des barres) | `scanner.rs` (`run_strategy`) |
| Rotation **collante** : une stratégie de secours qui vient de lire un code est gardée à la frame suivante, sinon la 2ᵉ lecture d'un code 1D arrive un cycle plus tard (12 → 6 frames sur la boucle) | `realtime/scheduler.rs` |
| Micro QR lu par le lecteur QR de rxing et rapporté comme `qr_code` | `result.rs` |
| Rotation des stratégies : une seule stratégie coûteuse par frame (×6,6 sur les frames vides) | `realtime/session.rs` |
| Les stratégies de secours se partagent le **temps CPU**, pas les frames : sur une scène texturée `Hard` coûte ~6× `Linear` ; à une frame chacune, un téléphone lent (Galaxy A14, ~5 fps) ne relançait `Linear` que toutes les ~1,5 s. Premier cycle dans l'ordre de la liste ; aucune stratégie n'attend plus de 3 cycles (anti-famine) | `realtime/scheduler.rs` |
| `Linear` décode **une ligne sur `row_step`** (le flou vertical rend les autres redondantes), avec les lecteurs 1D seuls et **sans** le réessai à 90° de rxing : ce réessai n'a lieu que si la source de luminance sait tourner, d'où `PlanarYUVLuminanceSource` (pas de rotation) au lieu de `Luma8LuminanceSource`. 69 → 19 ms sur une frame texturée | `decode/rxing_backend.rs` (`decode_rows`), `scanner.rs` (`decode_linear`) |
| `Linear` ne seuille **que les lignes décodées** (`linear_enhance_rows`, identique à `sample_rows(linear_enhance(..))`, testé) ; l'image complète n'est calculée que si un code est lu, pour la vérification de zone de silence. Min/max glissants par **van Herk / Gil-Werman** (3 comparaisons par pixel, ~4× plus rapide que les deques) | `preprocess/linear.rs`, `scanner.rs` (`decode_linear`) |
| CLAHE : tuiles de 8 px (≈ 6900 LUT pour 1024×432) ; le calcul des LUT dominait (14 ms sur 21) → une seule passe en `u32`, sortie identique à l'ancienne version (testé) | `preprocess/contrast.rs` |
| Sauvola par **sommes glissantes** (mêmes entiers que les images intégrales, donc même sortie, testé) : plus de tables `u64` de ~7 Mo par appel | `preprocess/binarize.rs` |
| Backend `auto` du grab : WebGL2, sauf si sa médiane dépasse 20 ms sur 15 frames ; Canvas 2D est alors mesuré et gardé seulement s'il est < 0,6 × WebGL (il bloque le thread principal). Décision unique, mesurée, sans liste d'appareils. Les scénarios e2e « webgl » forcent `backend: 'webgl'` (en headless le GPU est émulé et `auto` passerait en Canvas) | `src/grabber.ts` (`AdaptiveGrabber`), `test/unit/grabber.test.mjs` |
| Les scripts de build **vident** `src/wasm` et `dist/wasm` avant d'écrire : un fichier d'un ancien build ne doit pas partir dans le paquet | `scripts/build-wasm.mjs`, `scripts/copy-wasm.mjs` |
| Décodage **multi-workers** : l'état (ordonnanceur, confirmations, anti-doublons) doit exister en **un seul exemplaire**, sinon les confirmations 1D et la dé-duplication cassent entre workers → `ScanSession` sur le thread principal (`plan` / `commit`, quelques µs), workers sans état (`FrameDecoder`). Session et décodeurs doivent avoir la **même configuration** (un plan = un indice dans la liste des stratégies) | `realtime/decoder.rs`, `src/engine.ts` |
| Tout plan doit revenir à `commit`, même si le worker échoue (rapport vide) : sinon l'ordonnanceur le compte « en vol » pour toujours | `src/engine.ts` (`processFrame`) |
| Plans en vol comptés avec leur coût estimé : sans ça, 3 frames planifiées en même temps auraient la même stratégie | `realtime/scheduler.rs` |
| Rapports d'une configuration précédente (frames en vol pendant `configure`) ignorés via un numéro de génération | `src/engine.ts` (`generation`) |
| Moteur compilé **une fois** sur le thread principal (`WebAssembly.Module` envoyé aux workers) : un seul téléchargement / compilation | `src/core.ts` (`compileWasm`), `src/engine.ts` |
| 3 workers max par défaut (cœurs − 1) : au-delà le grab (~55 ms sur Galaxy A14, ~18 fps) limite et chaque worker coûte une instance WASM. Débit mesuré en e2e : ×3,25 avec 3 workers (4 cœurs) | `src/engine.ts` (`defaultWorkerCount`) |
| `BarcodeDetector` natif (option `processing.nativeDetector: 'auto'`, défaut `'off'` tant qu'il n'est pas mesuré sur appareil) = **passe rapide seulement**, lancé directement sur le `<video>` (Chrome lit l'image hors des threads de la page) : **pas de grab** sur ces frames. S'il trouve, ses codes passent par `ScanSession::commit_external` (même filtre de formats, mêmes confirmations 1D, même dé-duplication ; l'ordonnanceur n'est pas touché). S'il ne trouve rien, le moteur reçoit **de temps en temps** une frame sans passe rapide (`plan_fallback_only`, au plus une en vol, `FALLBACK_INTERVAL_MS` = 300 ms ; immédiate après une frame où le moteur voyait un code, pour la 2ᵉ lecture 1D) : filet de sécurité pour ce que le natif n'a pas (aucun prétraitement : gravure laser, ombre ; `micro_qr`). Grab + décodage à chaque frame en plus du natif doublait la lecture de l'image et occupait les workers pour rien (batterie, CPU dont le natif a aussi besoin). Coût mesuré en e2e : l'étiquette, lue seulement par les secours, sort en ~1,7 s au lieu de ~0,5 s ; quand le natif lit, 0 frame envoyée au moteur. Chaque décodeur n'efface que ses propres surlignages (sinon le natif effacerait ~30 fois/s un code lu par les secours) ; les frames de secours, une à la fois, sont numérotées à la fin du décodage. Utilisé seulement s'il couvre **tous** les formats demandés, `micro_qr` excepté ; `getSupportedFormats()` vide tant que Play Services n'a pas téléchargé son module → indisponible pour cette fois. Premier échec de `detect()` → abandonné pour le reader, avec un `console.warn` (comme le repli WebGL → Canvas). Pas de vérification de zone de silence sur ses codes 1D (il faudrait l'image) : seules les confirmations multi-frames filtrent ses faux positifs — à mesurer sur une texture avant d'en faire le défaut | `src/native.ts`, `src/reader.ts` (`acquire`, `present`), `realtime/session.rs`, `realtime/decoder.rs` (`skip_fast_path`), tests `native.test.mjs`, scénarios e2e `native-*` |
| Les décodages finissent dans le désordre : les surlignages ne sont mis à jour que par une frame plus récente que la dernière affichée | `src/reader.ts` (`seq`) |
| Throttle adaptatif désactivé par le SDK (le worker gère déjà la contre-pression) | `packages/scanner-web/src/core.ts` |
| Vue `frameBuffer()` invalidée à chaque croissance de la mémoire WASM : ne jamais la garder | `crates/scanner-wasm/src/lib.rs` |
| Pipeline à deux étages **en parallèle** : le grab (GPU) de la frame N+1 pendant le décodage (worker) de la frame N, sinon le temps par frame est `grab + decode` (Galaxy A14 : ~55 + ~125 ms). Au plus un grab en cours et une frame en attente : on saute des frames au lieu de les empiler | `src/reader.ts` (`tick`) |
| `start()` concurrents (double clic, mode strict React) : une seule promesse de démarrage partagée, sinon deux `getUserMedia` et un flux jamais libéré ; `stop()` pendant `start()` annule le démarrage (compteur `lifecycle`) et libère la caméra en cours d'ouverture. Scénario e2e « reader lifecycle » | `src/reader.ts` (`start`, `open`) |
| Page masquée (onglet en arrière-plan, écran verrouillé) : la caméra est **libérée** puis rouverte au retour (le décodage s'arrêtait déjà, pas la caméra : voyant allumé, batterie). Le reader reste « en cours » (pas d'événements `stopped` / `started`) ; échec de réouverture → `error` puis `stop()`. Désactivable : `camera.releaseWhenHidden: false`. Scénario e2e « camera released while hidden » | `src/reader.ts` (`onVisibilityChange`) |
| Erreurs de frame : seule la **première** d'une série est émise (`failures`), sinon un moteur indisponible émettrait ~30 erreurs/s ; rien n'est émis après `stop()` / `destroy()` | `src/reader.ts` (`handleFrameError`) |
| Un worker mort répond en erreur instantanément, donc paraît toujours le moins occupé : `pickWorker` l'exclut, sinon il recevrait la plupart des frames | `src/engine.ts` (`pickWorker`), `test/unit/engine.test.mjs` |
| Taille de buffer calculée en `u64` vérifié : `width * height` en `u32` reboucle en release (65536² = 0) et laissait passer une frame absurde jusqu'à une panique | `frame.rs` (`buffer_size`) |
| Trap WASM (panique Rust) = module dans un état indéfini → le worker est recréé | `src/worker.ts`, `src/engine.ts` |
| Seules les erreurs du pipeline d'image (`GrabError`) déclenchent le repli WebGL → Canvas 2D | `src/reader.ts` |
| Ligne 0 du framebuffer WebGL = haut de l'image (pas de flip) ; largeur de sortie multiple de 4 | `src/grabber.ts`, `src/geometry.ts` |
| Les objets `URL` ne passent pas `postMessage` → on ne transmet que les options moteur | `src/core.ts` (`pickEngineOptions`) |
| Un seul binaire WASM (pas de build `+simd128`) : l'auto-vectorisation n'apportait aucun gain mesurable sur ce décodeur (Node 22, frames texturées : 89 ms contre 84-93 ms) pour un build et un paquet deux fois plus gros. À reconsidérer seulement avec du SIMD écrit à la main dans les boucles chaudes, mesuré | `scripts/build-wasm.mjs` |
| Son de détection : l'`AudioContext` doit être créé pendant un geste utilisateur (iOS) | `src/feedback.ts`, `src/reader.ts` |
| Vite/Nuxt : `optimizeDeps.exclude` obligatoire, sinon les URL du worker et du `.wasm` cassent | `apps/demo-nuxt/nuxt.config.ts`, README du SDK |
| Tunnel Cloudflare : hôte `*.trycloudflare.com` autorisé ; QR code seulement si stdout est un terminal | `apps/demo-nuxt/nuxt.config.ts`, README de la démo |
| Pied de page de la démo : version du SDK **et** commit. Entre deux releases, `main` (donc la démo) contient du code plus récent que le numéro de version | `apps/demo-nuxt/nuxt.config.ts` (`runtimeConfig`) |
| Démo sur le domaine `scanner.romrd.fr` (racine, donc pas de `NUXT_APP_BASE_URL`). Le domaine se règle dans *Settings → Pages* ; le fichier `CNAME` est ignoré pour un déploiement Actions mais gardé par sécurité | `.github/workflows/pages.yml`, `apps/demo-nuxt/public/CNAME` |
| `allow-unwrap-in-tests` de clippy ne couvre pas `#[wasm_bindgen_test]` | `crates/scanner-wasm/tests/web.rs` |
| release-please met à jour `Cargo.toml` et `package.json` mais pas les lockfiles (qui recopient la version) → job dédié sur la PR de release | `.github/workflows/release.yml` (`sync-lockfiles`) |
| Publication en deux jobs : `package` (sans privilège : `npm ci`, build, `npm pack`) puis `publish` (seul à avoir `id-token: write`, n'installe et n'exécute **rien** venant de npm, publie le `.tgz`). Une dépendance de build compromise ne peut donc pas publier. Permissions définies job par job | `.github/workflows/release.yml` |
| tsify reste en 0.4 : la 0.5 déprécie `into_wasm_abi` / `from_wasm_abi` (fuite de la valeur JS quand la désérialisation échoue et lève une exception) au profit de `tsify::Ts`, soit une migration des bindings WASM. Le chemin normal, utilisé à chaque frame, ne fuit pas (mesuré : mémoire stable sur 22 000 frames). Dependabot ignore ses versions majeures en attendant | `crates/scanner-wasm`, `.github/dependabot.yml` |
| TypeScript épinglé en `~6.0` : `typescript-eslint` (peer `<6.1.0`), `vue-tsc` et `vite-plugin-checker` ne supportent pas TypeScript 7 (`npm ci` échoue en ERESOLVE). Dependabot ignore ses versions majeures, et celles de `@types/node` (on cible Node 22) | `package.json`, `.github/dependabot.yml` |
| Actions GitHub épinglées par **SHA de commit** (le tag en commentaire) et mises à jour par Dependabot (préfixes `ci` / `build` : pas de release déclenchée) ; `npm` épinglé dans le job de publication | `.github/workflows/*.yml`, `.github/dependabot.yml` |
| Les commits poussés avec `GITHUB_TOKEN` (PR de release) ne déclenchent aucun workflow → la CI est appelée par le workflow *Release* (`release-pr-ci`, `workflow_call`) sur la branche de release ; groupe de concurrence basé sur `inputs.ref` pour ne pas annuler la CI de `main` | `.github/workflows/release.yml`, `.github/workflows/ci.yml` |
| Publication npm par OIDC (trusted publishing) : npm ≥ 11.5.1 requis, Node 22 n'embarque que npm 10 | `.github/workflows/release.yml` |
| release-please réécrit `.release-please-manifest.json` en JSON compact à chaque release → exclu de Prettier (sinon `npm run lint` casse sur `main` après chaque release) | `.prettierignore` |
| npm refuse la provenance pour un dépôt **privé** (E422 « Unsupported … repository visibility ») → `--no-provenance` tant que le dépôt est privé, `--provenance` sinon ; le trusted publishing fonctionne dans les deux cas | `.github/workflows/release.yml` |
| `bootstrap-sha` de release-please = dernier commit de la 0.4.0 : sans lui, la première exécution (aucune release existante) reprendrait tout l'historique. Ne sert plus dès que le tag `v0.4.0` existe ; impose une fusion par *merge commit* (pas *squash*) | `release-please-config.json` |
| `wasm-opt` vient de la devDependency `binaryen` (dans le PATH via `npm run`) ; sans lui le build marche mais le `.wasm` est ~7 % plus gros | `packages/scanner-web/scripts/build-wasm.mjs` |

## Tests et fixtures

- `tests/fixtures/` est **versionné** : les tests ne dépendent pas de Python. On ne
  relance `generate.py` que pour ajouter ou modifier une image, puis on met à jour
  les valeurs attendues.
- Les cas les plus durs sont **synthétisés dans les tests** (encodeurs rxing, flou,
  bruit, textures) : c'est déterministe et ça couvre chaque format.
- `tests/fixtures/real/` contient les **vraies photos** : boucle bovine (Code 128
  gravé au laser) et étiquette d'aliment (Code 39 dans l'ombre du téléphone, lu
  seulement par `Linear` sur la photo entière). Chaque photo a sa valeur attendue et est testée en
  scan statique, en session et telle que la voit la caméra du SDK (recadrage ROI), plus
  un scénario e2e. Toute photo qui échoue chez un utilisateur doit finir ici.
- Ce qui manque encore : davantage de vraies photos (reflets, codes abîmés, angles).
