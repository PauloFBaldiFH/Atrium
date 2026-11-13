# Elzevir: definições de implementação

> Status: aprovado por Paulo em 2026-11-12, junto com o início da Fase 0. Complementa [04-decisoes.md](04-decisoes.md); onde houver diferença com os docs 01 a 03, vale este documento.

Cinco definições que faltavam para começar a escrever código sem retrabalho.

## 1. Interface da thread escritora: comandos tipados

O resto do sistema não envia funções para a escritora. Envia **comandos**: um `enum` por área, com os dados já validados, e recebe a resposta por um canal de volta.

```rust
pub enum WriteCommand {
    Circulation(CirculationCommand),
    Catalog(CatalogCommand),
    // ...
}

pub enum CirculationCommand {
    Checkout { patron: PatronId, item: ItemId, branch: BranchId, operator: StaffId, at: Timestamp },
    Checkin  { item: ItemId, branch: BranchId, operator: StaffId, at: Timestamp },
}

pub struct WriteRequest {
    pub command: WriteCommand,
    pub reply: oneshot::Sender<Result<WriteOutcome, WriteError>>,
}
```

Por quê:

- **Testável sem banco de verdade.** Um teste monta o comando e confere a decisão; o mesmo comando roda contra o oráculo do Koha nos cenários.
- **Auditoria de graça.** O comando, serializado, é o registro do `audit_log`: quem pediu o quê, quando. Reaplicar a sequência de comandos reproduz o estado.
- **Nada escapa da fila.** Uma função enviada poderia capturar qualquer coisa (conexões, `Arc`s, relógio). Um comando só carrega dados.

Regras:

- O "agora" (`at`) vem dentro do comando, decidido na borda. A escritora nunca lê o relógio.
- A fila é limitada. Fila cheia devolve `WriteError::Busy` para quem pediu, que responde 503 com `Retry-After`; nunca bloqueia o *runtime* assíncrono.
- A escritora agrupa comandos que chegam juntos numa só transação (até um limite de tamanho e de tempo), e cada comando recebe a própria resposta.

## 2. Várias instituições: uma escritora por instituição, com limites

Cada instituição tem a sua pasta de dados, a sua thread escritora e o seu pool de leitura. Com dezenas de instituições no mesmo servidor, isso precisa de limites:

| Recurso | Padrão | Configurável em |
|---|---|---|
| Instituições abertas ao mesmo tempo | 64 | `[servidor] instituicoes_abertas` |
| Conexões de leitura por instituição | 4 | `[banco] leitores` |
| Tempo sem uso até fechar uma instituição | 15 minutos | `[banco] fechar_apos` |

- Uma instituição é **aberta sob demanda** na primeira requisição para o seu endereço e **fechada** depois do tempo sem uso: a escritora termina a fila, faz `PRAGMA optimize` e um *checkpoint* do WAL, e solta as conexões.
- Se o limite de instituições abertas for atingido, fecha-se a que está há mais tempo sem uso (LRU).
- Tarefas agendadas de uma instituição fechada a reabrem no horário e a deixam fechar depois.
- O Tantivy segue a mesma regra: o índice de cada instituição abre e fecha junto com ela.

## 3. Pré-cálculo otimista com `row_version`: adiado

Na v1 a **escritora faz tudo**: lê, decide (chamando as funções puras do domínio) e grava, dentro da mesma transação. O pré-cálculo nas conexões de leitura com conferência de `row_version` (doc 03, §1.2) fica **fora** até um benchmark mostrar fila na escritora.

- A coluna `row_version` continua existindo desde a primeira migração: custa pouco e serve para cache de fragmentos por versão e para detectar edição concorrente em formulários.
- Gatilho para reabrir o assunto: p95 de espera na fila da escritora acima de 20 ms no benchmark de circulação, ou importação abaixo da meta de 2.000 registros/s por culpa da escritora.

## 4. Formatos de trabalho

### Spec

Uma spec por funcionalidade em `docs/specs/<area>/<nome>.md`, com estas seções, nesta ordem:

1. **Comportamento**: o que acontece, em frases curtas e verificáveis.
2. **Origem no Koha**: subs, telas e testes `.t` consultados, com caminho em `reference/koha/`.
3. **Paridade**: `exigida` ou `livre` (ver doc 04, §2). Se exigida, os cenários que a provam.
4. **Casos de borda**: lista numerada; cada um vira um teste ou cenário.
5. **Desvios**: diferenças intencionais, também registradas em `docs/desvios-conhecidos.md`.
6. **Fora do escopo**.

### Cenário diferencial (YAML)

Em `scenarios/<area>/<nome>.yaml`, executado contra o Elzevir e contra o oráculo gravado do Koha:

```yaml
id: circ-due-date-default-loan
spec: docs/specs/circulacao/vencimento.md
parity: required
given:
  patron: { category: B }
  item: { itemtype: BK, branch: MAIN }
when:
  - checkout: { branch: MAIN }
then:
  - loan: { due: recorded }
```

- O oráculo é uma instância real com **relógio real** (doc 04, §3): não dá para fixar a data. Por isso o cenário não traz `now`. Quem grava anota o instante da operação, e a gravação guarda esse instante.
- A gravação também guarda as **entradas efetivas** que o Koha usou: a regra de circulação resolvida para o contexto (`issuelength`, `lengthunit`, `daysmode`, `hardduedate` e afins), o calendário da biblioteca (dias da semana fechados, feriados fixos e datas avulsas) e as preferências que a sub consulta. O Elzevir refaz o cálculo a partir dessas entradas e deve chegar à mesma saída.
- `then` diz o que comparar; o valor esperado vem da gravação (`recorded`). Quando o cenário fixa um valor (por exemplo, uma recusa), ele precisa bater com a gravação e com o Elzevir.
- Nomes de regras e campos seguem o Koha (`issuelength`, `categorycode`) para o executor traduzir sem tabela de apelidos.
- Só dados fictícios ou o cadastro de exemplo autorizado; o gravador nunca lê outros leitores.

### Gravação do oráculo

- Arquivo `scenarios/<area>/<nome>.koha.json`, ao lado do cenário, com: versão do Koha (bate com o `koha-lock.json`), hash do cenário, instante da operação com fuso, entradas efetivas e saídas.
- O gravador (`elzevir-scenarios`, binário `oracle-record`) lê a URL e as credenciais de variáveis de ambiente, faz a operação pela API REST e pelas telas da equipe, e **desfaz o que fez** (devolve o exemplar, apaga regra criada).
- O CI comum nunca acessa a instância. Ele confere que cada gravação corresponde ao cenário atual (hash) e que o Elzevir reproduz as saídas.
- Cenário alterado exige gravação nova; sem acesso ao oráculo, o executor acusa a gravação desatualizada em vez de passar.

### Cenário convertido de teste do Koha (`.t.yaml`)

Os testes `.t` do Koha fixam todas as entradas e afirmam a saída com `is()`, então já são um oráculo e não precisam de gravação. Cada um vira `scenarios/<area>/<nome>.t.yaml`, com um caso por asserção:

```yaml
id: calcfine-t
spec: docs/specs/circulacao/multa.md
parity: required
source: t/db_dependent/Circulation/CalcFine.t   # relativo a reference/koha/
defaults:                                        # contexto comum a todos os casos
  CalculateFinesOnReturn: 1
  rule: { fine: "1.00", lengthunit: days, chargeperiod: 1 }
cases:
  - name: Test basic functionality               # a descrição do is() no Koha
    context: { rule: { overduefinescap: "0" } }  # mesclado por cima de defaults
    state: !on_loan { due: "2000-01-01T00:00" }  # ou: available
    event: !checkin { at: "2000-01-30T00:00" }   # !checkout, !renew, !checkin
    expect: { fine: "29.00" }                    # e/ou due: "AAAA-MM-DDTHH:MM"
```

- O caso roda contra a `transition()` pura do domínio; datas são civis, no fuso `timezone` do contexto (padrão UTC).
- O contexto usa os nomes do Koha: regras (`issuelength`, `renewalperiod`, `daysmode`, `fine`, `firstremind`...), preferências (`useDaysMode`, `ReturnBeforeExpiry`, `finesCalendar`...), `calendar` (`weekly_closed`, `annual`, `closed`, `open`) e `library_hours`.
- O teste do Koha muda preferências e feriados em sequência; cada caso escreve o estado em vigor na hora da asserção, e um comentário explica quando o `.t` depende de efeito colateral (por exemplo, uma data mutada).
- Âncoras YAML e `<<` servem para repetir contexto. O CI roda todos os casos (`crates/elzevir-scenarios/tests/koha_tests.rs`).

### Logs (`tracing`)

- Eventos estruturados, nunca texto montado: `info!(item = %item_id, branch = %branch, "checkout accepted")`.
- Mensagem em inglês, curta, no passado ou estado; os dados vão em campos.
- *Spans* por requisição HTTP, por comando da escritora e por tarefa agendada, com `institution` sempre presente.
- Níveis: `error` exige ação de alguém; `warn` é anomalia tratada; `info` é evento de negócio; `debug` e `trace` só para diagnóstico.
- **Nunca** registrar nome, documento, e-mail, telefone ou endereço de leitor, nem senhas e tokens. Leitor aparece só pelo ID interno.
- Saída em JSON em produção e legível no terminal em desenvolvimento.

## 5. Toolchain e lints

- **Rust fixado** em `rust-toolchain.toml` (1.97.0), **edition 2024**, `rust-version` igual no workspace. Atualizar a versão é um PR próprio.
- **Lints do workspace** em `Cargo.toml`, herdados por todas as crates com `[lints] workspace = true`:
  - `unsafe_code = "forbid"` em todas as crates. Se o `elzevir-marc` um dia precisar de `unsafe` (só com benchmark), ele troca os lints só dele, num PR que explique o motivo.
  - Proibidos em código de produção: `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `indexing_slicing`, `string_slice`, `exit`, `dbg_macro`, `print_stderr`, `float_arithmetic`.
  - `allow_attributes` e `allow_attributes_without_reason`: exceção só com `#[expect(lint, reason = "...")]`, que também falha quando deixa de ser necessária.
  - `clippy::pedantic` como aviso, e o CI roda com `-D warnings`.
  - Em testes, `unwrap`, `expect`, `panic` e indexação são livres (`clippy.toml`).
- **CI** (`.github/workflows/ci.yml`): `fmt`, `clippy`, `cargo xtask ci` (camadas e tamanho de arquivo) e `cargo-deny` no Linux; testes com `nextest` em Linux e Windows, cada um em x86_64 e ARM64. O macOS saiu da matriz em 2026-11-12 porque os runners são lentos; volta quando o projeto precisar validar a plataforma.
