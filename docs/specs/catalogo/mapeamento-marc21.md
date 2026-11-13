# Mapeamento MARC21 → modelo bibliográfico (v0)

> Status: v0, tarefa 1.6, parte 1 (importação). Base: [visão de arquitetura](../../ARCHITECTURE_VISION.md) (A.1, B.2 e B.4, aprovadas por Paulo em 2026-11-12), [modelo bibliográfico](modelo-bibliografico.md) e o framework MARC21 padrão do Koha 26.05.05 (`installer/data/mysql/en/marcflavour/marc21/mandatory/marc21_framework_DEFAULT.yml`). A exportação (`bibframe_to_marc`) e os testes de ida e volta com edição são a parte 2.

## 1. Comportamento

`marc_to_bibframe` recebe um registro MARC21 já em UTF-8 (tarefa 1.4) e devolve a Obra, a Instância e os Itens que ele descreve, mais o registro original para `source_records`. É uma função pura: não lê banco nem relógio. Nenhuma outra parte do Elzevir lê etiqueta MARC.

Regras gerais:

1. **Texto**: cada valor tem espaços nas pontas removidos e a pontuação ISBD final (`/`, `:`, `;`, `=`, `,`, com ou sem espaço antes, e `.` final que não seja de abreviatura de uma letra nem reticências) removida, como o Koha faz ao preencher `biblio.title` e `biblio.author`. Valor vazio depois disso é ignorado.
2. **Primeira ocorrência**: campo não repetível no modelo usa a primeira ocorrência no MARC.
3. Campo ou subcampo que não está nas tabelas abaixo não vira coluna; continua no registro original, que é guardado inteiro (B.2 item 1).

### 1.1 Categoria do material (Obra)

Do líder, posições 06 e 07:

| Líder 06 | Líder 07 | `MaterialType` |
|---|---|---|
| `a`, `t` | `b`, `i`, `s` | Periódico |
| `a`, `t` | outros | Livro |
| `e`, `f` | | Mapa |
| `c`, `d` | | Partitura |
| `i`, `j` | | Gravação sonora |
| `g` | | Vídeo |
| `m` | | Digital |
| `k`, `r` | | Material visual |
| `o`, `p` | | Misto |
| outro | | Livro |

A **Instância** recebe `Digital` quando a Obra não é `Digital` e o registro descreve um recurso eletrônico: `007/00` = `c`, ou `008/23` (forma do item, para livros, periódicos, partituras e misto) é `o`, `q` ou `s`. Senão, fica com a categoria da Obra.

### 1.2 Obra

| Modelo | MARC21 |
|---|---|
| título | `130$a`, senão `240$a`, senão `245$a` |
| classificação e esquema | `082$a` (`ddc`), senão `080$a` (`udc`), senão `050$a` (`lcc`) |
| resumo | `520$a` |
| idiomas | cada código de 3 letras em cada `041$a` (o Koha às vezes junta `porfre` num subcampo); sem `041`, `008/35-37` quando não é branco nem `|||` |
| contribuição principal | `100` (pessoa; família quando o 1º indicador é `3`), `110` (entidade), `111` (evento) |
| outras contribuições | `700`, `710`, `711`, na ordem |
| assuntos | `600`, `610`, `611` (agente), `630` (título), `648` (cronológico), `650` (tópico), `651` (lugar), `655` (gênero/forma), na ordem |

- **Cabeçalho de agente**: subcampos `a`, `b`, `c`, `d`, `q` (e `n` em `111`/`711`), unidos por espaço como estão e limpos só no fim (`Ginzberg, Eli, 1911-2002`).
- **Papel**: `$4` (código de relator) se houver; senão `$e` em minúsculas. Sem nenhum dos dois: sem papel.
- **Cabeçalho de assunto**: `$a` (e `$b`, `$c`, `$d`, `$t` dos assuntos de agente), seguido das subdivisões `$v`, `$x`, `$y`, `$z` com ` -- `, na ordem do campo.
- **Tesauro do assunto** pelo 2º indicador: `0` → `lcsh`, `1` → `lcac`, `2` → `mesh`, `3` → `nal`, `5` → `cash`, `6` → `rvm`, `7` → o valor de `$2`. Outros: sem tesauro.

Hub e relações entre Obras não são criados na importação desta versão (ADR 0002: só com teste próprio).

### 1.3 Instância

| Modelo | MARC21 |
|---|---|
| título | `245$a`, `$n`, `$p` e `$b`, nessa ordem dentro do campo, `$b` depois de ` : ` |
| edição | `250$a` |
| local, editora, data | primeiro `264` com 2º indicador `1`, senão `260`: `$a`, `$b`, `$c` |
| descrição física | `300` `$a`, `$b`, `$c` unidos por espaço, sem tirar a pontuação do meio |
| série | `490$a` (e ` ; ` + `$v`), senão `830$a`, senão `440$a` |
| ISBN | `020$a`; o que vem entre parênteses depois do número vira qualificador, e também `$q` |
| ISSN | `022$a` |
| LCCN | `010$a` |
| DOI, ISMN, EAN | `024$a` com 1º indicador `7` e `$2` = `doi`; `2`; `3` |
| número de controle | `035$a` |

### 1.4 Itens (`952`, convenção do Koha)

| Modelo | `952` | Koha |
|---|---|---|
| código de barras | `$p` | `barcode` |
| tombo | `$i` | `stocknumber` |
| unidade de origem | `$a` | `homebranch` |
| unidade atual | `$b` (sem ela, a de origem) | `holdingbranch` |
| localização | `$c` | `location` |
| coleção | `$8` | `ccode` |
| tipo de item | `$y`, senão `942$c` do registro | `itype` |
| número de chamada | `$o`; sem ele, `090$a` + `$b`, senão `082$a` (B.4) | `itemcallnumber` |
| número do exemplar | `$t` | `copynumber` |
| situação | `$7`, `$4`, `$1`, `$0`, `$5` (número, 0 = normal) | `notforloan`, `damaged`, `itemlost`, `withdrawn`, `restricted` |
| preço, preço de reposição | `$g`, `$v` (centavos; valor não numérico é ignorado) | `price`, `replacementprice` |
| data de aquisição | `$d` | `dateaccessioned` |

Os outros subcampos do `952` são dados que o Koha calcula ou mantém sozinho (`$9` número interno, `$6` ordenação do número de chamada, `$l`, `$m`, `$s`, `$r`, `$w`, `$q` de circulação e controle). Não viram colunas; ficam no registro original.

## 2. Origem no Koha

- `Koha::Item` e o framework MARC21 padrão (`kohafield` de cada subcampo do `952`).
- `C4::Biblio::TransformMarcToKoha` para a ideia de "primeira ocorrência" e da pontuação removida (`_disambiguate` e `_clean_punctuation` não são portados linha a linha).
- Corpus: os 200 primeiros registros da base de testes de Paulo, exportados pela tela da equipe em 2026-11-13 (`tools/export.pl`, ISO 2709 e MARCXML com exemplares).

## 3. Paridade

`livre` para o modelo; `exigida` para a exportação, que a parte 2 prova. Prova desta parte: snapshots `insta` do resultado do mapeamento para os registros de amostra, e testes de unidade para cada regra das tabelas.

## 4. Casos de borda

1. Registro sem `245`: título vazio é recusado com erro de mapeamento (`MappingError::MissingTitle`), não com pânico.
2. `952` sem `$a`: recusado (`MissingHomeBranch`), com a posição do campo.
3. `952` sem tipo de item e registro sem `942$c`: recusado (`MissingItemType`).
4. Preço com vírgula decimal (`12,50`) é lido; texto (`doação`) é ignorado.
5. `041$a` com tamanho que não é múltiplo de 3: os códigos completos entram; o resto é ignorado.
6. Agente repetido entre `100` e `700` com o mesmo cabeçalho: duas contribuições (o modelo deduplica o Agente, não a contribuição).

## 5. Desvios

Nenhum (área livre).

## 6. Fora do escopo

- Exportação e ida e volta com edição: parte 2 desta tarefa.
- Hub a partir de título uniforme e relações a partir de `76X`–`78X` (`775` aparece no corpus): depois, com teste próprio.
- Autoridades (`$0`, `$9`): fase seguinte.
- Registros UNIMARC: Fase 4.
