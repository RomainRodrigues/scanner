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

### Prérequis

- Rust (rustup + cargo) et Node.js 20+.
- La cible `wasm32-unknown-unknown` et `wasm-bindgen-cli` (à la version exacte de
  `Cargo.lock`) sont **installés automatiquement** au premier `npm run sdk` / `npm run build`.
- Optionnel : `npm install -g binaryen` (fournit `wasm-opt`, binaire ~20 % plus petit).
- Pour les tests Rust : `pip install qrcode pillow python-barcode` (images de test).

### Tests du moteur Rust

```bash
python3 tests/images/generate_test_images.py
cargo test --workspace --release
```

### Tests WASM (dans Node.js)

```bash
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p scanner-wasm --target wasm32-unknown-unknown --test web
```

### SDK et tests de bout en bout

```bash
cd packages/scanner-web
npm install
npm run build        # Rust → WASM (standard + SIMD) puis TypeScript → dist/
npm run test:e2e     # Chromium headless + fausse caméra (ffmpeg + Playwright)
```

### Démo Nuxt 4 et test sur téléphone

```bash
cd apps/demo-nuxt
npm run sdk          # construit le SDK (à relancer après modif du Rust ou du SDK)
npm install
npm run dev          # sur l'ordinateur : http://localhost:3000
npm run tunnel       # sur le téléphone : nuxt dev --tunnel
```

Sur téléphone la caméra exige du HTTPS. `npm run tunnel` (= `nuxt dev --tunnel`) lance le
serveur **et** un tunnel rapide Cloudflare en une seule commande : Nuxt affiche une URL
`https://xxx.trycloudflare.com` et un QR code à scanner avec le téléphone. La première fois,
Nuxt demande d'accepter les conditions de Cloudflare et télécharge `cloudflared` tout seul
(ou l'installer avant, ex. `brew install cloudflared`). Si le rechargement à chaud se comporte
mal à travers le tunnel : `npm run preview` (build de production servi sur le port 3000).

Le composant `app/components/BarcodeScanner.vue` est un wrapper réutilisable à copier
tel quel dans une vraie app Nuxt / Vue.

---

## 📦 Publier sur npm

```bash
cd packages/scanner-web
npm run build
npm publish --access public
```

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
