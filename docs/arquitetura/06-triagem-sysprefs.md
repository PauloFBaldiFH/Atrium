# Atrium: triagem das preferências do sistema do Koha (v0)

> Status: proposta (tarefa 0.11), para Paulo revisar. Data: 2026-11-12. Base: Koha 26.05.05.

O Koha 26.05.05 tem **915 preferências do sistema**. O Atrium não as copia: cada uma recebe um **destino**, e só vira opção do `atrium.toml` o que tiver justificativa (meta da v1: menos de 100 opções; doc 02, §1 item 5).

- Inventário extraído do Koha: [`data/koha-26.05/sysprefs.json`](../../data/koha-26.05/sysprefs.json) (nome, valor padrão, aba e seção da tela de preferências).
- Destino de cada uma: [`tools/sysprefs-triage.json`](../../tools/sysprefs-triage.json).
- `cargo xtask check-sysprefs` (no CI) falha se alguma preferência ficar sem destino, por exemplo quando uma versão nova do Koha acrescentar preferências.

## Destinos

| Destino | Significado | Quantas |
|---|---|---|
| `config` | Vira chave tipada do `atrium.toml` | 14 |
| `regra` | Vira dado editável (regra de circulação, calendário), não opção global | 2 |
| `fixo` | O Atrium adota um comportamento e não oferece opção | 30 |
| `interface` | Interface redesenhada: vira *token* de design ou deixa de existir | 36 |
| `plataforma` | Resolvido pela plataforma (painel, servidor, sessões, SMTP, estado interno) | 55 |
| `fora-v1` | Módulo que não entra na v1 | 129 |
| `revisar` | Decidido na spec do módulo, uma a uma | 649 |

## Por aba da tela de preferências

| Aba do Koha | Preferências | Destino padrão | Quando se decide o resto |
|---|---|---|---|
| Circulation | 241 | `revisar` (2 já viram `regra`: `useDaysMode`, `finesCalendar`) | Spec de circulação, Fase 3. Área de paridade exigida: cada uma conferida contra o oráculo |
| OPAC | 153 | `revisar` | Spec do OPAC: aparência vai para *tokens*, comportamento para `config` ou `fixo` |
| Patrons | 79 | `revisar` | Spec de leitores, Fase 2 |
| Enhanced content | 66 | `fora-v1` | Capas e resenhas de serviços externos, depois da v1 |
| Cataloging | 63 | `revisar` | Spec de catalogação, Fase 1 |
| Searching | 56 | `revisar` | Spec de busca, Fase 1 (a busca é Tantivy; muitas devem virar `fixo`) |
| Administration | 44 | `plataforma` | — |
| Staff interface | 36 | `interface` | — |
| Acquisitions | 29 | `fora-v1` | — |
| Logging | 26 | `fixo` | O `audit_log` registra toda escrita |
| Authorities | 23 | `revisar` | Spec de autoridades |
| (ocultas) | 17 | `plataforma` | — |
| Tools | 16 | `revisar` | Por ferramenta |
| Interlibrary loans, Serials | 14 cada | `fora-v1` | — |
| Web services | 13 | `revisar` | Spec de OAI-PMH, SRU e API |
| I18N/L10N | 10 | `config` | — |
| Accounting | 8 | `revisar` | Spec de multas e pagamentos, Fase 3 |
| E-resource management, Preservation | 4 e 3 | `fora-v1` | — |

## O que falta

Esta é a v0: os grupos inteiros já têm destino e o CI garante que nada fica esquecido, mas **649 preferências** são decididas uma a uma na spec do módulo correspondente, com o critério de entrar como `config` só com justificativa escrita. Os valores que a Biblioteca Municipal de Palotina mudou em relação ao padrão são o melhor ponto de partida para essa revisão; eles podem ser lidos da instância de testes quando ela estiver pronta (preferências não contêm dados de leitores).
