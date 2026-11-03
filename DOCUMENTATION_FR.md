# 📖 Guide du projet Scanner (v0.4)

Ce document explique l'architecture, les choix techniques face à STRICH, comment
utiliser le SDK et comment tout tester en local.

---

## 🏗️ Architecture

Le projet est recentré sur **une seule cible : la librairie npm pour les sites web clients**.
Le CLI (`scanner-cli`) et l'addon Node natif (`scanner-napi`) ont été supprimés.

```
crates/scanner-core/     Moteur Rust : prétraitements, stratégies de décodage, session temps réel
crates/scanner-wasm/     Bindings WebAssembly (API bas niveau)
packages/scanner-web/    SDK TypeScript publié sur npm : @gueshbzh-perso/scanner
apps/demo-nuxt/          Démo Nuxt 4 (test sur téléphone via tunnel Cloudflare)
```

Le client n'installe que le SDK : il embarque le WASM, le worker et l'UI.

```ts
import { BarcodeReader, scanImage } from '@gueshbzh-perso/scanner';

const reader = await BarcodeReader.create({ host: '#scanner', formats: ['qr_code', 'ean_13'] });
reader.on('detected', (codes) => console.log(codes[0].text));
await reader.start();          // depuis un clic (son autorisé sur iOS)

const codes = await scanImage(fichier);   // image fixe
```

---

## 🎯 Comment fonctionne STRICH, et ce qu'on a repris

STRICH est un SDK commercial (licence payante, clé vérifiée en ligne). D'après sa
documentation publique, il utilise **WebGL** pour traiter le flux caméra et
**WebAssembly** pour localiser et décoder les codes, entièrement côté client, avec
une UI prête à l'emploi (zone de scan, lampe, choix de caméra) et un `PopupScanner`.

Leur force ne vient pas d'un « meilleur décodeur magique » mais du **pipeline autour** :
ne jamais bloquer l'UI, ne donner au décodeur que les pixels utiles, et répartir
les traitements coûteux dans le temps. C'est ce qui manquait à l'ancienne version :

| Problème de la v0.3 | Ce qui a été fait en v0.4 |
| --- | --- |
| Décodage synchrone dans `requestAnimationFrame` → l'UI gèle | Décodage dans un **Web Worker**, une seule frame en vol (pas de file d'attente, latence constante) |
| `drawImage` + `getImageData` de toute la frame en RGBA (4 octets/px) | **Pipeline WebGL2** : recadrage ROI + niveaux de gris + réduction (filtre box) + 4 pixels par texel en un seul shader, relecture asynchrone (PBO + fence) de `largeur × hauteur` octets seulement. Repli Canvas 2D automatique |
| Chaque frame vide exécutait toute la cascade (rapide → CLAHE → Sauvola → inversion → inversion+Sauvola) | **Rotation des stratégies** : passe rapide + **une** stratégie de secours par frame, différente à chaque frame. Toutes sont couvertes en ~150 ms à 30 fps |
| Copies multiples des pixels | Écriture directe dans la mémoire WASM (`frameBuffer()`) |
| Points des codes faux avec une ROI (relatifs au recadrage) | Points toujours en coordonnées de la frame vidéo ; l'overlay les dessine au bon endroit |
| `format` renvoyé `"QR Code"` alors qu'on configure `"qr_code"` | Même orthographe en entrée et en sortie |
| `maxResults > 1` ignoré | Vrai mode multi-codes (lecteur multiple de rxing) |
| Faux positifs 1D (surtout **UPC-E**) en visant une scène sans code | **Vérification de la zone de silence** (marge blanche obligatoire autour d'un vrai code-barres, que rxing ne contrôle presque pas pour UPC/EAN) + **confirmation multi-frames** : 2 lectures concordantes pour les codes 1D, 3 pour les formats fragiles (UPC-E, EAN-8, Code 39/93, ITF, Codabar), 1 pour les codes 2D |
| Résultats `Unknown` (RSS, Telepen… non exposés) remontés quand aucun format n'est filtré | Liste de formats toujours explicite côté rxing, `Unknown` jamais renvoyé |
| `confidence` / `orientation` toujours à 1.0 / 0.0 | Supprimés (ils trompaient l'utilisateur) |
| Toutes les features par défaut de rxing (encodeurs, 15 codecs image, `chrono-tz`…) | Uniquement les décodeurs nécessaires |
| Pas de gestion caméra / UI : à recoder par chaque client | `BarcodeReader` : caméra arrière, mémoire de la caméra choisie, lampe torche, zoom, autofocus continu, changement de caméra, masque ROI, surlignage des codes, bip + vibration |
| Un seul binaire WASM | Deux binaires (standard + **SIMD**), choix automatique |

### Mesures

- Coût d'une frame **sans code** (le cas le plus fréquent), 1024×432, natif :
  **~168 ms → ~25 ms** par frame analysée (~6,6× plus rapide).
- Test e2e dans Chromium headless (caméra simulée) : premier code lu en ~150–350 ms
  après le démarrage de la caméra. Les temps GPU affichés en headless ne sont pas
  représentatifs (rendu WebGL émulé par le CPU via SwiftShader) : **il faut encore
  mesurer sur de vrais téléphones**.

### Ce qui reste pour vraiment jouer dans la même cour que STRICH

1. **Localisation sur GPU** : détecter les zones « qui ressemblent à un code-barres »
   (densité de gradients) dans le shader, puis ne décoder que ces zones. C'est le plus
   gros gain restant, surtout pour les petits codes loin de la caméra.
2. **Décodeur 1D par lignes de balayage** dédié (au lieu de rxing) : beaucoup plus
   robuste au flou et aux codes abîmés, c'est là que les SDK commerciaux se distinguent.
3. **Benchmark sur un vrai jeu d'images** (photos de codes réels, flous, reflets) pour
   comparer objectivement avec STRICH / ZXing / le `BarcodeDetector` natif.
4. Un `PopupScanner` (scan en un appel dans une `<dialog>`), et des wrappers React / Vue.
5. Taille du WASM (~1 Mo, ~450 Ko gzip) : retirer les formats inutilisés via des
   features Cargo par build.

---

## 💻 Lancer le projet en local

Prérequis : Rust (rustup + cargo) et Node.js 20+. La cible `wasm32-unknown-unknown`
et `wasm-bindgen-cli` (à la version exacte de `Cargo.lock`) sont **installés
automatiquement** au premier build, et `wasm-opt` (qui réduit le `.wasm` d'environ 7 %)
est installé par `npm install`.

Le dépôt utilise les **npm workspaces** : un seul `npm install`, à la racine, installe
le SDK et la démo.

```bash
npm install          # une fois, à la racine
npm run build        # Rust → WASM (standard + SIMD) + TypeScript (à relancer après modif du Rust ou du SDK)
npm run dev          # démo sur l'ordinateur : http://localhost:3000
npm run tunnel       # démo sur le téléphone : nuxt dev --tunnel (URL https + QR code)
```

`npm run sdk` depuis `apps/demo-nuxt` fait toujours la même chose que `npm run build`.

Sur téléphone la caméra exige du HTTPS. `npm run tunnel` (= `nuxt dev --tunnel`) lance le
serveur **et** un tunnel rapide Cloudflare : Nuxt affiche une URL
`https://xxx.trycloudflare.com` et un QR code (seulement si la commande tourne dans un vrai
terminal). La première fois, Nuxt demande d'accepter les conditions de Cloudflare et
télécharge `cloudflared` tout seul. Si le rechargement à chaud se comporte mal à travers le
tunnel : `npm run preview -w scanner-demo-nuxt` (build de production sur le port 3000).

Le composant `apps/demo-nuxt/app/components/BarcodeScanner.vue` est un wrapper réutilisable
à copier tel quel dans une vraie app Nuxt / Vue.

### Qualité et tests

```bash
npm run lint && npm run typecheck          # ESLint + Prettier, tsc + vue-tsc
npm run test:e2e                           # Chromium headless + fausse caméra (ffmpeg + Playwright)
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p scanner-wasm --target wasm32-unknown-unknown --test web
```

**Les images de test** (`tests/fixtures/`) sont versionnées : il y en a peu, c'est voulu.
Les cas difficiles sont **générés directement dans les tests** (encodeurs rxing + flou +
bruit + textures), ce qui est déterministe et couvre chaque format 1D/2D. Ce qui manque :
un jeu de **vraies photos** (reflets, codes abîmés, angles) pour un benchmark réaliste.

### GitHub Actions

- **CI** (`.github/workflows/ci.yml`, à chaque push sur `main` et chaque pull request) :
  fmt / clippy / tests / doc Rust, tests WASM dans Node.js, ESLint + Prettier, build du
  SDK, vérification des types, tests e2e avec fausse caméra, build de la démo.
- **Pages** (`.github/workflows/pages.yml`, à chaque push sur `main`) : génère la démo
  Nuxt et la publie sur https://romainrodrigues.github.io/scanner/.
  À activer une fois : *Settings → Pages → Build and deployment → Source : GitHub Actions*.

Règles de contribution, cas particuliers connus et conventions : [CLAUDE.md](CLAUDE.md).

---

## 📦 Versions et publication sur npm

**Rien à modifier à la main.** Il n'y a qu'une version pour tout le projet, gérée par
[release-please](https://github.com/googleapis/release-please) à partir des messages de
commit (Conventional Commits) :

1. À chaque push sur `main`, le workflow *Release* ouvre ou met à jour une **PR de release**
   (« chore(main): release 0.5.0 »). Elle calcule la prochaine version (`fix:` → patch,
   `feat:` → minor ; avant la 1.0, un changement cassant `feat!:` passe aussi en minor),
   met à jour `Cargo.toml`, `packages/scanner-web/package.json`, `CHANGELOG.md` et les
   lockfiles.
2. Tu relis la PR, tu la fusionnes quand tu veux publier → tag `v0.5.0`, release GitHub, et
   publication du SDK sur npm avec provenance.

**À configurer une fois :**

- Première publication (le paquet `@gueshbzh-perso/scanner` n'existe pas encore sur npm) :
  créer un token npm *Automation* (npmjs.com → Access Tokens), l'ajouter en secret
  `NPM_TOKEN` (GitHub → Settings → Secrets and variables → Actions), puis lancer à la main
  le workflow *Release* (onglet Actions → Release → Run workflow) : il publie la 0.4.0 et
  crée la release `v0.4.0`.
- Ensuite, recommandé : activer le **trusted publishing** sur npmjs.com (paquet → Settings →
  Trusted Publisher → GitHub Actions, dépôt `RomainRodrigues/scanner`, workflow
  `release.yml`, environnement `npm`) et supprimer le secret `NPM_TOKEN` : plus aucun
  token à faire tourner.
- Autoriser GitHub Actions à créer des PR : Settings → Actions → General → *Allow GitHub
  Actions to create and approve pull requests*.

Publication manuelle de secours : `npm run build && npm publish -w @gueshbzh-perso/scanner --access public`.

Côté client avec **Vite / Nuxt**, exclure le paquet du pré-bundling, sinon les URLs
du worker et du `.wasm` sont cassées :

```ts
// vite.config.ts
export default { optimizeDeps: { exclude: ['@gueshbzh-perso/scanner'] } };
```

Exemple Vue 3 (version complète : `apps/demo-nuxt/app/components/BarcodeScanner.vue`) :

```ts
let reader;
onMounted(async () => {
  reader = await BarcodeReader.create({ host: scannerEl.value, formats: ['ean_13'] });
  reader.on('detected', ([code]) => emit('scanned', code.text));
});
const start = () => reader.start();      // sur un clic
onUnmounted(() => reader?.destroy());    // libère caméra, worker et mémoire WASM
```

---

## ⚠️ Changements cassants depuis la 0.3

- Paquet client : `@gueshbzh-perso/scanner` (le paquet `scanner-wasm` reste l'API bas niveau).
- `ScanResult.format` est maintenant `"qr_code"`, `"ean_13"`… ; `points` est un tableau de `{ x, y }`.
- `confidence` et `orientation` ont disparu ; `timestamp_ms` devient `timestampMs`.
- Statistiques de session : un seul getter `stats` au lieu de plusieurs getters.
- `scanner-cli` et `scanner-napi` sont supprimés.
