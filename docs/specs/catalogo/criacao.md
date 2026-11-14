# Criação pela tela: "+ Nova" e catalogação contínua (v0)

> Status: v0, tarefa 2.6. Base: [spec de interação](../../UI_BIBFRAME_INTERACTION_SPEC.md) §6.1, §6.2 e §20 (itens 5, 7 e 9), [edição](edicao.md) e [modelo bibliográfico](modelo-bibliografico.md). Critério da tarefa: **registro criado pela tela exporta MARC válido e entra no índice**.

A tarefa 2.6 tem duas partes, cada uma num PR:

- **2.6a, escrita:** o comando `catalog.add_instance` e as rotas que criam uma família nova ou uma Instância nova de uma Obra existente (itens 1 a 12).
- **2.6b, tela:** "+ Nova" na coluna Instância, `Ctrl+N` para família nova, o bloco novo já em edição e a catalogação contínua com herança do registro anterior (itens 13 a 20).

Ficam para depois: Obra nova no mesmo Hub e a sugestão "Agrupar num Hub" (§20 item 5), que dependem de editar Hub; e a linha de exemplar nova, que é da grade (2.7).

## 1. Comportamento

### Comando

1. `catalog.add_instance` (`CatalogCommand::AddInstance`) acrescenta uma Instância a uma Obra existente, sem Itens. A categoria da Instância, quando não vem, é a da Obra (como na inclusão de registro).
2. Obra inexistente: `CatalogRefusal::UnknownWork`, e nada é gravado.
3. A Instância nova ganha as chaves de casamento de ISBN e ISSN dos seus identificadores (como na inclusão) e entra na fila do índice (`catalog.reindex_instance`). A Obra e as outras Instâncias dela não mudam de versão: a página delas não mostra as irmãs.
4. Família nova é o `catalog.add_record` que já existe, sem registro de origem e sem Itens.

### Rotas

5. `POST /equipe/acervo/nova` cria uma família nova (Obra e a primeira Instância). Corpo: `{"work": [...], "instance": [...]}`, com as mudanças no mesmo formato da edição ([edição](edicao.md), item 16), aplicadas sobre uma Obra e uma Instância vazias.
6. `POST /equipe/acervo/obra/{id}/instancias` cria uma Instância nova da Obra `{id}`. Corpo: `{"changes": [...]}`, mudanças de Instância.
7. As duas rotas pedem o mesmo que a edição: sessão (401), cabeçalho CSRF e permissão `cataloguing` (403) ([edição](edicao.md), item 17). O `audit_log` guarda quem criou.
8. A validação é a da edição ([edição](edicao.md), item 19), mais o mínimo para existir (spec de UI §20, item 9), recusado de uma vez com 422 e uma mensagem por campo:
   - título da Obra (família nova) e título da Instância;
   - categoria do material da Obra (família nova). A categoria nunca é adivinhada: a tela manda a da sessão.
9. Na família nova, se o título da Instância não vem, ela herda o título da Obra (spec de UI §6.1: a Instância herda título, categoria e idioma; o idioma é da Obra).
10. Criação aceita responde 201 com `{"instance": id}`, para a tela focar a família nova. Obra inexistente responde 404.
11. A conversão das mudanças num registro novo é uma função pura da camada app (`creating.rs`), testada sozinha; a rota só a chama.
12. Campo opcional vazio nunca impede criar (§20, item 9).

### Tela (2.6b)

13. Para quem cataloga, cada bloco de Obra tem **"+ Instância"**, que abre o formulário de uma Instância nova daquela Obra, com o título e a categoria da primeira Instância dela (spec de UI §6.1) e o cursor no primeiro campo vazio do perfil. O botão fica na Obra, e não no cabeçalho da coluna Instância, porque uma família com Hub tem várias Obras. O formulário abre logo abaixo dos blocos da família.
14. A barra tem **"+ Nova família"**, e a tecla `N` (fora de um campo) faz o mesmo: sem família aberta, abre uma família nova vazia, com blocos de Obra e Instância já em edição e a categoria da sessão (a última usada; livro na primeira vez); com família aberta, abre uma Instância nova da Obra em foco (ou da primeira Obra). `Ctrl+N` faz o mesmo onde o navegador deixa a página usá-lo; o Chrome o reserva para nova janela e não o entrega à página, por isso `N` é o atalho garantido.
15. O bloco novo usa o mesmo formulário e o mesmo perfil da edição; **Criar** (`Ctrl+Enter`) grava; **Cancelar** e `Esc` fecham o formulário.
16. Bloco novo com algo digitado segue a regra da edição pendente ([edição](edicao.md), item 32): nada some sem "Descartar".
17. **Catalogação contínua** (§6.2, §20 item 7): depois de criar, `N` (ou "+ Nova família") abre outra família nova com editora, local, data, idiomas, assuntos e categoria do registro anterior, cada campo herdado marcado **"herdado"** até ser confirmado ou trocado.
18. Confirmar um campo herdado é editá-lo ou passar por ele com `Tab`; a marca some. Um campo herdado é enviado como qualquer outro.
19. A herança fica no `sessionStorage` da aba (a última Instância criada e a categoria): vale enquanto a aba estiver aberta, mesmo depois que a tela vai para a família criada, e some ao fechar a aba. O formulário pede ao servidor `GET /equipe/acervo/nova?from={instância}`, e o servidor monta os valores herdados a partir do registro gravado.
20. Criar uma família leva a tela para a página da listagem que começa nela, já aberta (`/equipe/acervo?after=...#f-w{id}`); criar uma Instância recarrega a família aberta.
21. Os formulários vêm do servidor: `GET /equipe/acervo/nova?material=&from=` (família) e `GET /equipe/acervo/instancia/{id}/nova` (Instância da Obra de `{id}`), só para quem tem a permissão `cataloguing` (sem ela: 403).

## 2. Origem no Koha

- Inclusão: `C4::Biblio::AddBiblio` (chamada por `cataloguing/addbiblio.pl`).
- Catalogação contínua: o Koha não tem; o mais próximo é o modelo de catalogação ("framework" com valores padrão) e a duplicação de registro (`addbiblio.pl?op=duplicate`).

## 3. Paridade

`livre`: criação é interface e modelo interno. O registro criado pela tela não tem registro de origem; a exportação o monta só pelo mapeamento ([mapeamento](mapeamento-marc21.md)), e o teste confere que o MARC sai válido.

Testes: `crates/elzevir-app/src/creating.rs` (conversão e validação), `crates/elzevir/tests/creating.rs` (comando, chaves, índice e exportação, de ponta a ponta) e `crates/elzevir-web/tests/staff_catalog.rs` (rotas).
