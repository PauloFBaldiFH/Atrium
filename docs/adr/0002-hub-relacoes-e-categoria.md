# ADR 0002: Hub, relações entre Obras, categoria do material e tombo

- **Status:** aceito (Paulo aprovou a [visão de arquitetura](../ARCHITECTURE_VISION.md) na íntegra em 2026-11-12, seção B.3). Complementa o [ADR 0001](0001-modelo-bibliografico.md).
- **Spec:** [modelo-bibliografico.md](../specs/catalogo/modelo-bibliografico.md), itens 14 a 18.

## Contexto

A visão de arquitetura pede a cadeia completa do BIBFRAME 2.0 (Hub → Obra → Instância → Item), relações semânticas entre Obras, uma categoria de material que escolha o ícone e o perfil de campos da catalogação, e o tombo dos exemplares. O esquema da migração 3 tinha Obra, Instância e Item, mas nada disso.

## Decisão

1. **Tabela `hubs`** e coluna opcional `works.hub_id`. Uma Obra pertence a no máximo um Hub. Na importação, o Hub só nasce quando a tarefa 1.6 decidir com teste (título uniforme `130`/`240`); fora disso, é criado à mão.
2. **Tabela `work_relations`** com o tipo como `enum` estável (`WorkRelationKind`), guardada num sentido só (a linha fica na tradução, na adaptação, na parte). O sentido inverso ("tem tradução") é uma consulta pelo índice `work_relations_by_related`, não uma segunda linha.
3. **`material_type` como `enum` estável** (`MaterialType`, 9 categorias) na Obra e na Instância. A Instância sem categoria própria recebe a da Obra no momento da gravação. A categoria vem do líder e dos campos `008`/`007` na importação, nunca do tipo de item do Koha (tipo de item é regra de circulação, não descrição).
4. **`items.inventory_number`** (tombo, `952$i`), opcional e sem unicidade, como o `stocknumber` do Koha.
5. **`IdentifierKind::Doi` e `Ismn`.**
6. Tudo por **migração nova** (4), sem editar a 3. Por isso as colunas novas de `works` e `instances` aceitam nulo (o SQLite não acrescenta coluna `NOT NULL` sem valor padrão); os comandos sempre gravam a categoria, e a leitura devolve `None` só para linhas anteriores à migração.

## Consequências

- O canvas de nós da Onda 2 e a API de grafo leem Hub e relações por junções numéricas.
- O perfil de campos da catalogação (Onda 1) depende só de `material_type`.
- Apagar uma Obra citada por outra é recusado pelo banco; apagar a Obra que cita leva as relações dela.

## Alternativas descartadas

- **Categoria única por registro, só na Obra:** não representa a edição digital de um livro impresso.
- **Relações nos dois sentidos:** duplica dados e pode divergir.
- **Usar o tipo de item como categoria:** mistura regra de empréstimo com descrição; uma biblioteca pode ter vários tipos de item para livros.
