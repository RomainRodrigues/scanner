# Security policy

## Supported versions

Only the latest release of `@gueshbzh-perso/scanner` receives fixes. The
project is before 1.0: upgrade to the latest version before reporting.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately through GitHub:
[**Report a vulnerability**](https://github.com/RomainRodrigues/scanner/security/advisories/new)
(Security tab → *Report a vulnerability*). Include the version, the browser
and device, and the steps or the image that trigger the problem.

You should get an answer within a week. Once a fix is released, the advisory
is published with credit to the reporter, unless you prefer otherwise.

## Scope

- The SDK and its WebAssembly engine, as published on npm.
- The release pipeline (`.github/workflows`): the package is published from
  GitHub Actions with npm trusted publishing (OIDC, no stored token) and
  provenance, from the `main` branch only.

The demo site (scanner.romrd.fr) is a static page built from this repository:
report issues with it the same way.
