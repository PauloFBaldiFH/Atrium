# reference/

Código de **referência de leitura**, fora do workspace Cargo. Nada aqui é compilado, empacotado ou distribuído com o Atrium.

| Caminho | O que é | Versão fixada |
|---|---|---|
| `koha/` | Koha upstream ([Koha-Community/Koha](https://github.com/Koha-Community/Koha)), GPLv3, como submódulo git raso | tag `v26.05.05-1`, commit `4f25ee4dced64026e546e047a0b7cbb371902e10` (`Koha.pm`: `26.05.05.000`) |

Para baixar:

```sh
git submodule update --init --depth 1 reference/koha
```

Para trocar de versão, atualize o submódulo para a nova tag num PR próprio e, a partir da tarefa 0.3, regenere o `koha-lock.json`. A versão-alvo é a estável mais recente do Koha (ver [docs/arquitetura/04-decisoes.md](../docs/arquitetura/04-decisoes.md), §3).
