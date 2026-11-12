# reference/

Código de **referência de leitura**, fora do workspace Cargo. Nada aqui é compilado, empacotado ou distribuído com o Atrium.

| Caminho | O que é | Versão fixada |
|---|---|---|
| `koha/` | Koha upstream ([Koha-Community/Koha](https://github.com/Koha-Community/Koha)), GPLv3, como submódulo git raso | tag `v26.05.05-1`, commit `4f25ee4dced64026e546e047a0b7cbb371902e10` (`Koha.pm`: `26.05.05.000`) |

Para baixar:

```sh
git submodule update --init --depth 1 reference/koha
```

Para trocar de versão, num PR próprio:

1. aponte o submódulo para a nova tag e atualize `koha-lock.json` (versão, série, tag e commit);
2. rode `perl tools/koha-extractor/extract.pl` para regenerar `data/koha-<série>/`;
3. rode `cargo xtask codegen` para regenerar `crates/atrium-data/src/generated/`;
4. revise o diff dos JSON: cada mudança de comportamento do Koha aparece ali.

O CI confere os passos 2 e 3 (`extract.pl --check` com o submódulo, e `cargo xtask codegen --check`). A versão-alvo é a estável mais recente do Koha (ver [docs/arquitetura/04-decisoes.md](../docs/arquitetura/04-decisoes.md), §3).
