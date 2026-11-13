# Casamento de registros na importação (v0)

> Status: v0, tarefa 1.10 (parte 1). Base: [doc 03](../../arquitetura/03-estrategia-reconstrucao.md) §3.2 (regras padrão do Koha), [importação](importacao.md) e [modelo bibliográfico](modelo-bibliografico.md). Critério da tarefa: **reimportar o mesmo arquivo não duplica**.

## 1. Comportamento

### Regras de casamento

1. Uma importação pode usar **uma** regra de casamento, ou nenhuma (o padrão, como no Koha: "não procurar registros iguais"). Sem regra, a importação faz exatamente o que a [importação](importacao.md) já fazia.
2. As regras são as três padrão do Koha (`marc21_default_matching_rules.yml`), cada uma com um ponto de casamento de peso 1000 e limiar 1000, ou seja: basta uma chave igual.

| Regra | Chaves do registro que chega | Chaves de um registro guardado |
|---|---|---|
| `Isbn` | cada ISBN da Instância (020 `$a`), normalizado | os mesmos, guardados na gravação |
| `Issn` | cada ISSN da Instância (022 `$a`), normalizado | idem |
| `KohaBiblionumber` | `999 $c` do registro de origem (o `biblionumber` de uma exportação do Koha) | o `999 $c` do registro de origem guardado |

3. **Normalização do ISBN** (Koha: `Koha::Util::Normalize::ISBN`, que chama `C4::Koha::NormalizeISBN` com `ISBN-13` e sem hífens): ficam os algarismos e o `X`; um ISBN-10 com dígito verificador válido vira ISBN-13 (`978` + 9 algarismos + novo dígito); um ISBN-13 válido (`978` ou `979`) fica como está. Um ISBN inválido vira a chave sem hífens nem espaços e em maiúsculas.
4. **ISSN**: sem hífens nem espaços, em maiúsculas.
5. **Koha biblionumber**: o primeiro `999 $c`, aparado; só algarismos contam como chave.
6. As chaves de todo registro gravado (por importação ou pela catalogação) ficam na tabela `match_keys (instance_id, rule, key)`, indexada por `(rule, key)`, escritas na mesma transação do registro. A migração 5 cria a tabela e calcula as chaves dos registros que já existiam.

### Decisão por registro

7. O casamento é feito **pela escritora, no momento de gravar**, na mesma transação. Por isso um registro casa também com um registro anterior do **mesmo arquivo**: no Koha, a etapa de preparação compara só com o acervo e duas cópias no mesmo arquivo viram dois registros. Aqui viram um (área livre; melhora o critério da tarefa).
8. Com vários candidatos, vale a Instância de **menor id** (a mais antiga). O Koha ordena os candidatos pelo número do registro como texto, em ordem decrescente; não há motivo para copiar isso.
9. Ação **quando casa** (`on_match`): `Ignore` (o registro guardado fica como está) ou `CreateNew` (grava um registro novo mesmo assim). `Replace` (sobrescrever o registro guardado) é a parte 2 da tarefa.
10. Ação **quando não casa** (`on_no_match`): `CreateNew` (padrão) ou `Ignore` (não grava).
11. **Exemplares** (`items`), como `C4::ImportBatch::_get_commit_action`:

| `items` | Registro casou | Registro não casou |
|---|---|---|
| `AlwaysAdd` (padrão) | acrescenta à Instância casada | grava com o registro novo |
| `AddOnlyForMatches` | acrescenta à Instância casada | não grava |
| `AddOnlyForNew` | não grava | grava com o registro novo |
| `Ignore` | não grava | não grava |

   Com `on_match = CreateNew`, os exemplares seguem a coluna "casou", mas vão para o registro novo, como no Koha.
12. **Exemplar a exemplar**: com uma regra de casamento, cada exemplar é gravado ou recusado sozinho, como em `_batchCommitItems`. Código de barras que já existe é recusado ("código de barras repetido") e os outros exemplares do registro entram. Unidade ou tipo de item desconhecido também recusa só aquele exemplar. Sem regra de casamento, vale a regra da importação: o registro inteiro é recusado.
13. Acrescentar exemplar a uma Instância casada sobe a versão do registro e gera reindexação, como `AddItem`.

### Relatório

14. O relatório da importação passa a ter, além de lidos, gravados e exemplares gravados:
    - **casados**: cada registro que casou, com sua posição no arquivo e a Instância casada (mesmo quando nada foi acrescentado a ela, ou quando um registro novo foi gravado com `on_match = CreateNew`);
    - **pulados**: registros sem par não gravados (`on_no_match = Ignore`);
    - **exemplares recusados**: posição do registro, posição do exemplar no registro e motivo.
15. Um exemplar recusado não deixa nada para trás: cada exemplar é gravado num ponto de salvamento próprio, desfeito na recusa.

## 2. Origem no Koha

- Regras: `installer/data/mysql/en/marcflavour/marc21/optional/marc21_default_matching_rules.yml`; pontuação e limiar: `C4::Matcher::get_matches`.
- Chaves: `C4::Matcher::_get_match_keys`, normalização `ISBN` (`Koha::Util::Normalize::ISBN`).
- Ações: `C4::ImportBatch::_get_commit_action` e `BatchCommitRecords`; exemplares: `_batchCommitItems`.
- O Koha prepara a importação em tabelas (`import_batches`, `import_records`) e casa na preparação. O Elzevir casa na gravação, sem tabelas de preparação.
- `AggressiveMatchOnISBN` (variações com e sem hífen, ISBN-10 e 13 na busca) fica coberto pela normalização para ISBN-13 dos dois lados.
- `Business::ISBN` também valida os intervalos de grupo e editora; o Elzevir confere só o dígito verificador.

## 3. Paridade

`livre` (importação não está na lista de paridade exigida), com o Koha como referência. Prova:

- `crates/elzevir-domain`: normalização de ISBN (ISBN-10 com `X`, ISBN-13, inválidos, hífens e espaços).
- `crates/elzevir-app`: decisão por ação (tabela do item 11) e chaves de um registro, incluindo `999 $c`.
- `crates/elzevir-store/tests/matching.rs`, com os 200 registros da base de testes:
  - **reimportar o mesmo arquivo com `KohaBiblionumber` não grava nenhum registro nem exemplar novo**: obras, Instâncias, exemplares, registros de origem, chaves e versões ficam iguais (só o `audit_log` registra os comandos); os 253 exemplares são recusados um a um por código de barras repetido;
  - com `Isbn`, só os registros sem ISBN são gravados de novo;
  - `AddOnlyForMatches` acrescenta à Instância casada só os exemplares com código de barras novo, e sobe a versão de cada Instância que ganhou exemplar;
  - `on_match = CreateNew` grava o registro novo com os exemplares;
  - sem regra, a importação continua gravando tudo;
  - `on_no_match = Ignore` não grava registro sem par;
  - duas cópias do mesmo registro no mesmo arquivo viram um registro;
  - a migração 5 calcula as chaves de uma base criada antes dela.

## 4. Fora desta parte

- `on_match = Replace` e `items = Replace` (sobrescrever registro e exemplares): parte 2, com o comando de substituição do registro.
- Regras definidas pela biblioteca (pontos de casamento e verificações obrigatórias de `C4::Matcher`).
- Casamento de autoridades.
