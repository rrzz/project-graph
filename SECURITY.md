# Security Policy

## Supported versions

Security fixes are applied to the latest release.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability that could expose source
content, bypass blocked paths, escape the project root, or corrupt a trusted
evidence lock. Contact the repository maintainer privately through the
security-reporting mechanism configured on the hosting repository.

Include reproduction steps, affected versions, impact, and any suggested
mitigation. Please allow reasonable time for investigation before disclosure.

## Data handling

Project Graph reads files referenced by assertions. Configure `blocked_paths`
conservatively. Do not store credentials, secret values, personal data, or
unreviewed model output in graph assertions. Generated SQLite files can contain
quoted evidence spans and should receive the same access controls as source.
