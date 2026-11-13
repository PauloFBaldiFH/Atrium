# CLAUDE.md

Instruções para quem escreve código no Elzevir (pessoas e agentes). Leia inteiro antes de mexer em qualquer coisa.

## O projeto em uma frase

O Elzevir é um sistema de bibliotecas em Rust que reconstrói o **comportamento** do Koha a partir de especificações e testes contra o Koha real, sem traduzir Perl, e melhora onde o Koha é ruim (interface, configuração, busca).

## Estado atual

- **Fase 0 concluída** (PRs #6 a #14). **Fase 1 em andamento** (autorizada por Paulo em 2026-11-12): MARC, modelo bibliográfico, busca e o executor de cenários. O oráculo é a instância de testes de Paulo (doc 04, §3); as credenciais ficam só em variáveis de ambiente de quem grava.
- Fontes de verdade, nesta ordem: [`docs/arquitetura/04-decisoes.md`](docs/arquitetura/04-decisoes.md) e [`05-definicoes-de-implementacao.md`](docs/arquitetura/05-definicoes-de-implementacao.md) > 03 > 02 > 01. Se o código contradiz o doc 04, o código está errado ou o doc 04 precisa de um PR primeiro.
- **Nome: Elzevir** (Paulo, 2026-11-12). O projeto se chamava Atrium; documentos e commits antigos ainda usam o nome anterior.
- **Antes de mexer no núcleo BIBFRAME (Hub, Work, Instance, Item), no mapeamento MARC21/Koha ou no frontend, leia [`docs/ARCHITECTURE_VISION.md`](docs/ARCHITECTURE_VISION.md)**: a especificação de Paulo (regra de ouro: nenhuma etiqueta MARC no núcleo, MARC só na borda, ida e volta sem perdas) e o plano de integração com o código, com a ordem das etapas.
- **Antes de mexer na interface do acervo (listagem, busca, catalogação, grade de exemplares, mesclagem), leia [`docs/UI_BIBFRAME_INTERACTION_SPEC.md`](docs/UI_BIBFRAME_INTERACTION_SPEC.md)**: o contrato de interação (foco 80/20, cordinhas, perfis por categoria, comandos por ação).
- `reference/koha/` é **referência de leitura** (GPLv3, submódulo fixado no Koha 26.05.05), não código do Elzevir, e fica fora do workspace Cargo. Baixe com `git submodule update --init --depth 1 reference/koha`.

## Como trabalhar numa tarefa

1. **Entenda o comportamento antes de escrever.** Leia o código do Koha, os testes `.t` dele e, quando houver, o oráculo gravado. Escreva em uma frase o que a função faz, para quais entradas, com quais casos de borda.
2. **Spec primeiro.** Mudou comportamento? Atualize ou crie a spec em `docs/specs/` antes do código.
3. **Teste que falha primeiro.** Cenário diferencial (área de paridade) ou teste de unidade (resto). Só então implemente.
4. **Implemente o mínimo** que faz o teste passar com o desenho certo. Nada de "já que estou aqui".
5. **Registre a origem.** Função que porta comportamento do Koha leva `/// Koha: <Modulo::sub>` no doc-comment. Diferença intencional numa área de paridade vai para `docs/desvios-conhecidos.md`.
6. **Rode as verificações** (seção "Antes de abrir PR") e só diga que algo funciona depois de ver passar.

Se a tarefa é ambígua, escolha a leitura mais provável, diga qual escolheu e siga. Se a escolha muda o objetivo, pergunte.

## Paridade com o Koha

- **Paridade exigida:** datas de vencimento e calendário, multas, fila de reservas, exportação MARC, SIP2, Z39.50. Aqui o Koha é o oráculo; qualquer diferença é bug ou desvio documentado.
- **Liberdade para melhorar:** interface, configuração (`elzevir.toml` tipado em vez de preferências soltas) e busca.
- Nunca traduza Perl linha a linha. Porte a regra, não a estrutura. Se o Koha tem um bug observável numa área de paridade, reproduza-o e documente o desvio que queremos fazer depois.

## Regras de Rust

### Desenho

- **Domínio puro.** `elzevir-domain` não faz I/O: não lê relógio, banco, rede, disco nem variável de ambiente. "Agora", calendário e regras entram como parâmetro.
- **Estados impossíveis não compilam.** `enum` com dados em vez de `bool`s e `Option`s combinados; *newtypes* para IDs (`ItemId(i64)`, nunca `i64` solto) e para valores com regra (`Barcode`, `Money`).
- **Dinheiro é inteiro** em centavos (`Money(i64)`), nunca `f64`.
- **Datas** com `jiff`; fuso da biblioteca explícito; nada de `Local::now()` fora da borda.
- **Uma regra, um lugar.** Resolução de regras de circulação numa única função pura. Não duplique decisão de negócio em handler, template ou SQL.
- **Camadas** (`tools/layers.json`, checado no CI): web → app → domain; store e search implementam portas de app. A web nunca toca o banco.
- **Escrita no SQLite só pela thread escritora.** Leitura pelo pool. Nenhuma outra conexão de escrita, nunca.

### Erros

- Erros tipados por módulo com `thiserror`; recusas de negócio são variantes com dados (`CheckoutRefusal::DebtOverLimit { owed, limit }`).
- `anyhow` só no binário e em `xtask`/ferramentas.
- Proibido em código de produção: `unwrap()`, `expect()` sem invariante provada no texto, `panic!`, `todo!`, `unimplemented!`, indexação que pode estourar (`v[i]` sem prova). Em testes, `unwrap` é livre.
- Não engula erro (`let _ =`, `.ok()`) sem comentário dizendo por quê.

### Segurança e dependências

- `#![forbid(unsafe_code)]` em domain, app, web e templates. `unsafe` só em `elzevir-marc`, num bloco documentado, e só se um benchmark provou o ganho.
- SQL sempre parametrizado. Nunca `format!` para montar SQL com dado de fora.
- Toda entrada externa (MARC, MARCXML, formulário, SIP2, Z39.50) tem limites explícitos de tamanho e profundidade.
- Nova dependência precisa de motivo no PR, licença compatível com GPLv3 e passar no `cargo-deny`. Prefira a biblioteca padrão.

### Estilo

- Código, identificadores, comentários e mensagens de log **em inglês**. Textos de interface só via Fluent (inglês primeiro, depois pt-BR); nenhuma string fixa em template.
- Funções curtas com um propósito; arquivo com mais de 3.000 linhas falha no CI.
- Nomes dizem o que é, sem abreviação inventada (`due_date`, não `dd`). Use o vocabulário de biblioteca (BIBFRAME, MARC, circulação) de forma consistente.
- Comentário explica **por quê**, não o quê. Doc-comment em todo item público.
- Sem código morto, `println!` de depuração, `#[allow]` sem justificativa ou TODO sem issue.
- `clippy` com `-D warnings` e os lints `pedantic` que o workspace habilitar.

### Desempenho

- Exato e simples por padrão. Otimização (arena, mmap, cache, SIMD) só com benchmark `criterion` mostrando o ganho, no mesmo PR.
- Evite cópia e alocação em laços quentes (parser MARC, importação, indexação): empreste (`&str`, `Cow`) em vez de clonar.

## Testes

- Domínio: testes de unidade e `proptest` para invariantes (ex.: vencimento nunca cai em feriado).
- Paridade: cenários YAML contra o oráculo gravado do Koha. O CI comum usa a gravação; não suba o Koha para isso.
- Snapshots com `insta` para mapeamentos MARC → modelo e para HTML.
- Parsers têm alvo de fuzzing.
- Teste nunca depende de relógio real, rede ou ordem de execução. Nunca desative, ignore ou afrouxe um teste para ficar verde.

## Antes de abrir PR

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo xtask check-layers
cargo xtask codegen --check
```

(`cargo xtask ci` roda as três verificações `xtask` de uma vez. Ao mexer em `reference/koha` ou `koha-lock.json`, rode também `perl tools/koha-extractor/extract.pl --check`.)

Definition of Done: spec atualizada se mudou comportamento; testes do nível certo; tudo acima verde; ADR em `docs/adr/` se tomou decisão de arquitetura; sem regressão de desempenho.

PRs pequenos (até ~500 linhas), um assunto por PR. Commits no formato Conventional Commits, em português: `feat(circ): ...`, `fix(marc): ...`. Arquivos-fonte com cabeçalho `// SPDX-License-Identifier: GPL-3.0-or-later`.

## Dados, privacidade e segredos

- Nunca coloque no repositório: backup do Koha, dados de leitores, senhas, tokens ou endereços de instâncias com credenciais.
- Instâncias de teste do Koha são descartáveis e separadas da produção. Não leia dados de leitores além do cadastro de exemplo; se aparecerem dados reais, pare e avise.
- Fixtures de teste usam leitores fictícios.

## Como escrever (código, docs e respostas)

- Objetivo e verificável: diga o que fez, o que testou e o resultado. Se não testou, diga.
- Não invente API, crate, função do Koha ou número. Confira no código ou na documentação antes de citar.
- Documentação do projeto em português; código em inglês.
