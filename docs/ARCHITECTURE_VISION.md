# Visão de arquitetura: núcleo BIBFRAME 2.0, borda MARC21/Koha e frontend

> Status: aprovada por Paulo como direção em 2026-11-12 (21:59 BRT), junto com o novo nome do projeto, **Elzevir**. A parte A é a especificação dele, sem cortes de conteúdo. A parte B é o plano de integração com o código existente. Onde B diverge de A, a divergência está dita e justificada; Paulo aprovou o plano na íntegra, com as escolhas de B.2, em 2026-11-12 (22:07 BRT).
>
> Quando aplicar: na tarefa 1.5 (parte 3) e 1.6 (núcleo e mapeamento), 1.8 e 1.9 (busca e listagem), 1.11 (round-trip), Fase 2 (Onda 1) e depois dela (Onda 2). Nada aqui interrompe a tarefa em andamento.

## Parte A. Especificação (Paulo, 2026-11-12)

### Ordem obrigatória antes de implementar BIBFRAME 2.0, borda MARC21/Koha ou frontend

1. **Planejamento integrado:** ler o estado do repositório e fazer um plano técnico que conecte esta especificação às peças existentes, sem quebrar o que funciona e sem duplicar lógica (parte B).
2. **Memória persistente:** guardar esta especificação e o plano na memória do projeto e ligá-los no `CLAUDE.md`.
3. **Execução na etapa correta:** aplicar o plano quando chegarmos ao núcleo BIBFRAME 2.0, à conversão MARC21/Koha e às Ondas 1 e 2 do frontend.

### A.1 Regra de ouro do modelo de dados: núcleo BIBFRAME 2.0 puro, MARC21 só na borda

- **Núcleo 100% BIBFRAME 2.0 no SQLite.** Nenhuma tabela principal tem nome de etiqueta MARC (proibido `marc_245`, `tag_952`). A cadeia é **Hub** (opcional: traduções, adaptações, títulos uniformes) → **Work** (obra) → **Instance** (edição) → **Item** (exemplar físico ou digital).
- **Entidades normalizadas.** Agentes (autores, tradutores, ilustradores, entidades), assuntos e identificadores (ISBN, ISSN, DOI) em tabelas próprias, ligados por tabelas de relacionamento com papéis semânticos (`written_by`, `translated_by`, `has_edition`, `exemplar_at`, `translation_of`).
- **Categoria do material (`material_type`).** Cada Work/Instance registra a categoria (livro, mapa, partitura, audiovisual, digital, periódico). Ela define o ícone e o **perfil de campos** da catalogação: 12 a 20 campos relevantes por perfil, nunca formulários de 300 campos.
- **MARC21 e Koha só na fronteira, com importação e exportação sem perdas.**
  - O MARC21 existe só no parser e no serializador (`marc_to_bibframe` e `bibframe_to_marc`).
  - Convenções do Koha mapeadas explicitamente:
    - número de chamada: `082$a` / `090$a` e `952$o` → `call_number`;
    - exemplares (`952`): `$p` → código de barras, `$a`/`$b` → unidade e localização (ver B.2, correção), `$y` → tipo de item, `$t` → número do exemplar;
    - autoria (`100`/`110`/`700`/`710` com relator `$e`/`$4`), títulos (`245`, `240`, `130`), publicação (`260`/`264`), idioma (`008`/`041`), ISBN (`020$a`) e assuntos (`650`/`651`).
  - Campos MARC sem mapeamento ficam preservados (`raw_marc_extra`, JSON ou MARCXML) para ida e volta sem perda.

### A.2 API, busca e circulação

- **Respostas abaixo de 1 ms:** JSON enxuto para navegação em grafo sob demanda (só a vizinhança imediata do nó, nunca o acervo inteiro).
- **Listagem de balcão:** a busca (Tantivy) devolve já na primeira linha título, autor principal, **número de chamada**, categoria do material, localização e exemplares disponíveis/emprestados.
- **Circulação só no Item:** empréstimos, reservas e leitores se ligam ao Item, nunca à Work ou Instance.
- **Privacidade (LGPD):** o OPAC nunca expõe leitores nem empréstimos nos endpoints do grafo. Na equipe, a camada Item → Empréstimo → Leitor fica oculta por padrão e só aparece com filtro autorizado.

### A.3 Frontend (sem frameworks pesados: HTML semântico, CSS moderno, View Transitions API, SVG nativo, JS modular; 60–120 FPS)

**Onda 1: balcão, listagem e grade de exemplares**

1. Listagem com número de chamada em destaque, ícone da categoria e expansão dos exemplares na própria linha.
2. Ficha que mostra **só os campos preenchidos** no modo leitura.
3. Edição **no lugar**: o bloco se expande, sem modal nem troca de página, mostrando os campos do perfil intercalados com os preenchidos. Fluxo de teclado (Ctrl+N) para catalogação contínua em lote.
4. **Grade de exemplares estilo planilha** na Instance: tombo, código de barras, número de chamada, localização, situação; ver, filtrar, cadastrar e editar dezenas de uma vez.

**Onda 2: canvas horizontal de nós ("cordinhas")**

1. Colunas da esquerda para a direita: Autoridades/Assuntos → Hub → Work → Instance → grade de Items, com rolagem e arraste horizontal.
2. Ligações em SVG com curvas Bézier e rótulos em português simples ("escrito por", "tradução de", "edição", "exemplares").
3. Ao clicar num nó, os blocos seguintes deslizam e as cordinhas acompanham, sem recarregar a página.

### A.4 Critério de validação

1. Plano de integração apresentado e salvo (este documento e o `CLAUDE.md`).
2. Esquema SQLite e parsers MARC21 auditados contra A.1.
3. `cargo test` com testes obrigatórios de ida e volta: exemplares `952`, números de chamada `082`/`090` e acentuação UTF-8.
4. A Onda 1 só começa com o núcleo BIBFRAME 2.0 e os testes em Rust 100% verdes.

## Parte B. Plano de integração com o repositório

### B.1 O que já existe e já obedece à regra de ouro

Auditoria feita em 2026-11-12 sobre `main` + PR #21 + a parte 2 da tarefa 1.5 (local):

| Peça | Onde | Situação diante de A.1 |
|---|---|---|
| Work, Instance, Item, Agente, Assunto, identificadores | `crates/elzevir-store/src/schema.rs` (migração 3), `crates/elzevir-domain/src/catalog.rs` | Já são tabelas tipadas sem nome de etiqueta MARC. Item tem `call_number`, `copy_number`, unidades, tipo, localização, coleção, situação, preços. |
| Papéis das contribuições | `contributions.role` → `terms` (vocabulário relator) | Já semântico: guarda o código de relator (`aut`, `trl`, `ill`); o rótulo "escrito por" vem da tradução do código. |
| MARC só na borda | `crates/elzevir-marc` (leitor ISO 2709, escrita ISO 2709/MARCXML, MARC-8/ISO 5426) | Já isolado: só `elzevir-store` e `elzevir-app` podem depender dele (`tools/layers.json`); o domínio não conhece MARC. |
| Registro preservado | `source_records` (MARC original, byte a byte, por Instância) | Cobre o objetivo de `raw_marc_extra` com folga (ver B.2). |
| Escrita única e leitura em pool | `elzevir-store` (thread escritora + `ReadPool`) | Base das respostas rápidas do grafo. |
| Circulação no Item | `due_date.rs`, `calendar.rs` (domínio puro) | Ainda sem tabela de empréstimo; quando vier (Fase 3), referencia `items.id`. |

Nada do que existe precisa ser desfeito. O plano **acrescenta**.

### B.2 Divergências e correções (para Paulo confirmar)

1. **`raw_marc_extra`.** Em vez de uma coluna nova com os campos não mapeados, o Elzevir mantém o MARC original inteiro em `source_records` e, na exportação de um registro editado, regenera os campos mapeados a partir do modelo e copia os demais do original. O conjunto de etiquetas mapeadas fica num só lugar no código (o mapeamento da tarefa 1.6). Motivo: uma coluna separada duplicaria dados e teria de ser mantida em sincronia; o original inteiro também permite exportar sem nenhuma diferença o registro que não foi editado. Se Paulo preferir a coluna explícita, ela é derivável a qualquer momento.
2. **`952$a`/`$b`.** No Koha, `952$a` é a unidade de origem (`homebranch`), `952$b` a unidade atual (`holdingbranch`) e `952$c` a localização na estante (`location`). O mapeamento segue o Koha: `$a` → `home_branch`, `$b` → `holding_branch`, `$c` → `location`.
3. **Tombo.** No Koha o tombo costuma ir em `952$i` (`stocknumber`, "número de inventário"). Entra como coluna `inventory_number` em `items`.
4. **Nomes dos papéis.** Os relacionamentos usam códigos estáveis em inglês no banco (`translation_of`, relator `trl`); a interface mostra "tradução de", "traduzido por" pelo Fluent, como manda o `CLAUDE.md`.

### B.3 Mudanças no núcleo (tarefa 1.5, parte 3: migração 4, antes da 1.6; feita em 2026-11-12, [ADR 0002](adr/0002-hub-relacoes-e-categoria.md))

1. **`hubs`**: `id`, `title`, `extra`, `row_version`; `works.hub_id` opcional (`REFERENCES hubs`). Na v1 nenhum registro importado cria Hub sozinho; Hub nasce de título uniforme (`130`/`240`) só quando a 1.6 decidir com teste, ou à mão.
2. **`work_relations`**: `(work_id, kind, related_work_id, position)` com `kind` como `enum` estável no domínio (`TranslationOf`, `AdaptationOf`, `HasPart`...).
3. **`material_type`** como `enum` estável no domínio (`Book`, `Map`, `Score`, `Audiovisual`, `Digital`, `Periodical`, ...) em `works` e `instances`. Na importação vem do líder 6/7 e do `008`/`007` (tarefa 1.6), não do tipo de item do Koha.
4. **`IdentifierKind::Doi`** (e o que mais a 1.6 encontrar, como `ISMN`).
5. **`items.inventory_number`** (tombo).
6. Comandos de escrita e leitura da parte 2 ganham esses campos; testes de esquema como os de `catalog_tests`.

Spec afetada: `docs/specs/catalogo/modelo-bibliografico.md`; decisão: novo ADR 0002 (Hub, relações entre obras e categoria de material).

### B.4 Borda MARC21/Koha (tarefas 1.6, 1.10 e 1.11)

- **Onde mora:** módulo de mapeamento em `elzevir-app` (função pura `marc_to_bibframe(&Record) -> NewRecord + Vec<NewItem>`) e o inverso `bibframe_to_marc`. O nome segue a especificação; nenhuma outra crate lê etiqueta MARC.
- **Tabela de mapeamento** explícita, testada com snapshots `insta` (já previsto na 1.6), cobrindo a lista de A.1 e as correções de B.2.
- **Testes de ida e volta obrigatórios** (A.4 item 3), em `cargo test`:
  - registro com `952` repetido (vários exemplares), `082` e `090`, acentuação UTF-8 (`ç`, `ã`, `é`) e registro MARC-8 convertido;
  - importar → exportar sem editar: igual byte a byte ao original;
  - importar → editar um campo mapeado → exportar: só aquele campo muda.
  - A fixture com `952` vem da exportação da equipe do oráculo (precisa da VM de volta), com leitor fictício.

### B.5 API, busca e circulação

- **Grafo (JSON):** rotas em `elzevir-web` (`/api/v1/nodes/{tipo}/{id}` devolve o nó e a vizinhança imediata), lendo por uma porta nova em `elzevir-app` (`GraphReader`) implementada em `elzevir-store`. Respeita as camadas: a web não toca o banco.
  - Meta de desempenho: < 1 ms no servidor para a vizinhança de um nó, medida com `criterion`, conforme a regra de otimizar só com benchmark.
- **Listagem de balcão (1.8/1.9):** o esquema Tantivy guarda título, autor principal, número de chamada, categoria e localização; a contagem disponível/emprestado vem do SQLite numa consulta por página (já previsto na 1.9). Tela de balcão na Fase 2.
- **Circulação (Fase 3):** `loans` e `holds` referenciam `items.id`. Nenhuma coluna de circulação em `works`/`instances`.
- **LGPD:** a porta do grafo para o OPAC não tem método que alcance leitor ou empréstimo (o tipo não deixa); na equipe, a camada de circulação é uma porta separada exigindo a permissão de circulação.

### B.6 Frontend

- **Coerente com a decisão atual** (doc 01 §2.3, doc 04): renderização no servidor com Askama + HTMX e "ilhas" pequenas de JS puro. A especificação acrescenta View Transitions, SVG nativo e JS modular, todos sem framework. Nenhuma mudança de stack.
- **Onda 1 = Fase 2** (catálogo e editor). Perfis de campos por `material_type` como dados tipados (um arquivo por perfil, 12–20 campos), lidos pelo editor. Grade de exemplares como Web Component em JS puro, salvando por comandos `catalog.*` em lote.
- **Onda 2:** depois da Onda 1, sobre a API do grafo (B.5).
- **Pré-condição** (A.4 item 4): só começa com a parte 3 da 1.5, a 1.6 e os testes de ida e volta verdes.

A interação detalhada das Ondas 1 e 2 (listagem, foco 80/20, edição no lugar, grade, mesclagem e quarentena) está em [`UI_BIBFRAME_INTERACTION_SPEC.md`](UI_BIBFRAME_INTERACTION_SPEC.md).

### B.7 Renomear para Elzevir

Feito em 2026-11-12, depois da parte 2 da tarefa 1.5: crates `atrium-*` → `elzevir-*`, binário `elzevir`, `atrium.toml` → `elzevir.toml`, variáveis `ATRIUM_*` → `ELZEVIR_*`, textos e documentação. Paulo renomeou o repositório no GitHub para `Elzevir`.

### B.8 Ordem

1. PR #21 (modelo, parte 1) → parte 2 (comandos e leitura) → **renomear para Elzevir**.
2. Tarefa 1.5 parte 3 (B.3) com ADR 0002.
3. Tarefa 1.6 com o mapeamento de B.4, já com os testes de ida e volta.
4. Segue a Fase 1 (1.7 a 1.12) com B.5 aplicado em 1.8/1.9.
5. Fase 2 com a Onda 1; Onda 2 em seguida.
