# ADR 0001: modelo bibliográfico relacional inspirado em BIBFRAME

- **Status:** aceito (decisões 4 e 5 do [doc 04](../arquitetura/04-decisoes.md), Paulo, 2026-11-12). Este registro detalha a forma das tabelas, proposta na tarefa 1.5.
- **Spec:** [modelo-bibliografico.md](../specs/catalogo/modelo-bibliografico.md).

## Contexto

O Koha guarda o registro bibliográfico como MARCXML e extrai dele algumas colunas. Isso prende telas, busca e relatórios ao MARC. O BIBFRAME 2.0 separa Obra, Instância e Item, mas é um vocabulário RDF, e a conversão MARC → BIBFRAME → MARC perde informação.

## Decisão

1. **Tabelas tipadas para Obra, Instância, Item, Agente e Assunto**, sem triple store. BIBFRAME em RDF é só formato de exportação.
2. **O MARC original fica guardado** por Instância, byte a byte. Exportar um registro não editado devolve o original.
3. **Uma Obra por registro importado na v1.** O esquema aceita várias Instâncias por Obra; fundir Obras será uma ferramenta com revisão humana.
4. **Vocabulários como identidades numéricas.** Unidades e tipos de item em tabelas próprias; os demais códigos numa tabela `terms` por vocabulário; tipos fixos como `enum` com número estável.
5. **Agentes e Assuntos são entidades únicas** por tipo e cabeçalho, ligadas às Obras por tabelas de ligação ordenadas.
6. **Colunas para o que é consultado; `extra` em JSON para o resto.**

## Consequências

- Listar "tudo deste autor" ou "tudo deste assunto" é uma junção por número, sem ler MARC.
- A importação precisa resolver cabeçalhos e códigos para números (um lote por vez, tarefa 1.7).
- Editar um registro no Elzevir exige regenerar o MARC a partir do modelo para os campos que ele entende (tarefa 1.6); até lá só o caminho "não editado" é sem perdas.
- Sem controle de autoridade na v1, cabeçalhos com grafias diferentes viram Agentes diferentes.

## Alternativas descartadas

- **MARC no núcleo, como o Koha:** mantém o problema que motivou o projeto.
- **Triple store RDF no SQLite:** consultas simples ficam lentas e o código difícil de ler.
- **Uma coluna de texto por código** (idioma, tipo de item): repete texto em milhões de linhas e deixa renomear um código caro.
