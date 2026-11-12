# Desvios conhecidos

Diferenças **intencionais** entre o Atrium e o Koha 26.05.05 nas áreas de paridade exigida (datas e calendário, multas, fila de reservas, exportação MARC, SIP2, Z39.50; ver [04-decisoes.md](arquitetura/04-decisoes.md), §2).

Regra: o Atrium é exato por padrão. Um cenário diferencial só pode divergir do oráculo do Koha se o desvio estiver listado aqui. O executor de cenários (tarefa 1.0) lê este arquivo, aceita a divergência de um cenário listado e rejeita qualquer outra; `cargo xtask check-deviations` (no CI) valida o formato.

## Formato

Cada desvio é uma seção `## DEV-NNNN: título` com **um** bloco `deviation` de chaves simples, seguido do motivo em prosa:

````text
## DEV-0001: Vencimento não cai em feriado cadastrado depois do empréstimo

```deviation
id: DEV-0001
area: circulacao
koha: C4::Circulation::CalcDateDue
scenarios: scenarios/circulacao/vencimento-feriado-posterior.yaml
decided_by: Paulo
date: 2026-11-12
```

Por que divergimos, o que o Koha faz, o que o Atrium faz e como isso aparece para a biblioteca.
````

| Chave | Obrigatória | Conteúdo |
|---|---|---|
| `id` | sim | `DEV-` + 4 dígitos, igual ao título, em ordem crescente |
| `area` | sim | `circulacao`, `multas`, `reservas`, `marc`, `sip2` ou `z3950` |
| `koha` | sim | Sub(s) do Koha afetadas, separadas por vírgula |
| `scenarios` | sim | Cenário(s) que divergem, separados por vírgula |
| `decided_by` | sim | Quem aprovou |
| `date` | sim | `AAAA-MM-DD` |

## Desvios

Nenhum até agora.
