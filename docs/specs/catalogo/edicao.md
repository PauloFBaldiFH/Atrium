# Edição de Obra e Instância (v0)

> Status: v0, tarefa 2.2. Base: [spec de interação](../../UI_BIBFRAME_INTERACTION_SPEC.md) §9, [modelo bibliográfico](modelo-bibliografico.md) e [mapeamento MARC21](mapeamento-marc21.md). Critério da tarefa: **editar e exportar muda o MARC só nos campos editados; edição concorrente é recusada**.

## 1. Comportamento

### Comandos

1. `catalog.update_work` (`CatalogCommand::UpdateWork`) muda partes de uma Obra; `catalog.update_instance` (`CatalogCommand::UpdateInstance`) muda partes de uma Instância. Os dois passam pela thread escritora como qualquer `WriteCommand::Catalog` e ficam no `audit_log` com essa `kind`.
2. Cada edição traz a lista de mudanças, uma por parte, como `enum` com dados:
   - Obra (`WorkChange`): título preferido, categoria do material, classificação (número e esquema juntos), resumo, público-alvo, idiomas, contribuições e assuntos.
   - Instância (`InstanceChange`): título, categoria do material, edição, local, editora, data, descrição física, série, identificadores e notas.
3. Listas (idiomas, contribuições, assuntos, identificadores, notas) são trocadas inteiras, na nova ordem. Agente, Assunto e termo de vocabulário existentes são reaproveitados, como na inclusão.
4. Se a mesma parte aparece duas vezes na lista, vale a última.
5. O que a edição não cita fica como está: Hub, relações entre Obras, Itens e o registro de origem.

### Versão e concorrência

6. A edição de Obra cita o `row_version` da Obra (`InstanceView::work_version`) e a de Instância o `row_version` da Instância (`InstanceView::version`), lidos quando a tela abriu.
7. Se a versão guardada for outra, o comando é recusado com `CatalogRefusal::Stale { read, current }` e nada é gravado: nem a mudança, nem o `audit_log`, nem efeito. A tela mostra "alterado por outra pessoa; recarregar".
8. A versão da Instância é a do registro inteiro: sobe quando a Instância, a Obra dela ou um Item dela muda (é a mesma usada pelo cache do OPAC). Por isso uma edição de Instância aberta antes de qualquer dessas mudanças é recusada. A versão da Obra só sobe quando a própria Obra muda.
9. Edição aceita sobe a versão de quem foi editado e responde com a nova (`CatalogOutput::WorkUpdated` ou `InstanceUpdated`), para a tela seguir editando sem reler.
10. Editar a Obra sobe também a versão de **todas** as Instâncias dela e enfileira `catalog.reindex_instance` para cada uma, porque a página e o MARC delas mostram a Obra. Editar a Instância faz o mesmo só para ela.
11. Lista de mudanças vazia: aceita se a versão confere, sem gravar nada e sem subir a versão.
12. Obra ou Instância inexistente: `UnknownWork` ou `UnknownInstance`.

### Exportação e casamento

13. O registro de origem não muda. A exportação parte dele e reescreve só os campos que o modelo mapeia ([mapeamento](mapeamento-marc21.md) §1.5), então um campo não editado sai como entrou e os outros registros saem byte a byte.
14. Trocar os identificadores refaz as chaves de casamento de ISBN e ISSN daquela Instância. A chave do `biblionumber` do Koha vem do registro de origem e fica.

### Permissão

15. Todo comando de catálogo exige a permissão `cataloguing` (`WriteCommand::permission`); contas de equipe exigem `administration`. A checagem é feita onde o pedido da pessoa vira comando (rotas da equipe, tarefa 2.5); a escritora confia no que recebe. `elzevir staff add --cataloguing` concede a permissão.

### Edição pela tela (tarefa 2.5)

A tarefa 2.5 tem três partes, cada uma num PR:

- **2.5a, escrita:** as rotas da equipe que viram comando, com quem editou no `audit_log`, validação e recusas por campo (itens 16 a 24).
- **2.5b, formulário:** o bloco focado vira formulário no lugar, com os perfis livro e periódico e "Campos ainda não preenchidos" (spec de UI §4.1 e §4.2).
- **2.5c, listas:** agentes, assuntos, idiomas e identificadores como *chips*, com autocompletar nas entidades existentes (spec de UI §4.1, itens 5 e 6).

16. `POST /equipe/acervo/obra/{id}` edita a Obra e `POST /equipe/acervo/instancia/{id}` edita a Instância. O corpo é JSON: `{"version": n, "changes": [...]}`, com uma mudança por parte, no mesmo formato do comando gravado no `audit_log` (`{"Title": "..."}`, `{"Languages": ["por"]}`, `{"Summary": null}` para apagar). Assim o que a tela manda é o que fica registrado.
17. A rota exige sessão da equipe (sem sessão: 401), o cabeçalho `X-Elzevir-Csrf` igual ao token da sessão (diferente: 403) e a permissão `cataloguing` (sem ela: 403). A checagem do item 15 é esta.
18. O `audit_log` guarda a conta que editou (`audit_log.actor`, migração 9). Comandos que não vêm de uma pessoa da equipe (importação, linha de comando) ficam com `actor` vazio.
19. Antes do comando, a validação confere o conteúdo e recusa tudo de uma vez, sem gravar nada, com uma mensagem por campo (resposta 422, `{"errors": [{"field": "...", "message": "..."}]}`):
    - título da Obra e da Instância não ficam vazios (espaços nas pontas são tirados);
    - ISBN tem 10 ou 13 dígitos com dígito verificador certo (hífens e espaços são ignorados e não gravados);
    - ISSN tem 8 caracteres com dígito verificador certo e é gravado como `NNNN-NNNC`;
    - código de idioma tem três letras minúsculas (MARC/ISO 639-2);
    - texto vazio num campo opcional apaga o campo.
20. `Stale` responde 409 com `{"current": n}`, e a tela mostra "alterado por outra pessoa; recarregar" sem fechar o formulário. Obra ou Instância inexistente responde 404.
21. Edição aceita responde 200 com `{"version": n}` (a nova versão de quem foi editado) e o bloco relê os campos.
22. Os textos das mensagens vêm do Fluent; o JSON leva a chave da mensagem e o texto já no idioma da interface.
23. A validação é uma função pura da camada app (`validate_work_changes`, `validate_instance_changes`), testada sozinha; a rota só a chama.
24. Edição da mesma parte duas vezes na mesma lista segue o item 4: vale a última.

## 2. Origem no Koha

- Edição: `C4::Biblio::ModBiblio` (chamada pelo editor `cataloguing/addbiblio.pl`), que regrava o registro MARC inteiro a partir do formulário.
- Permissão: `editcatalogue` (`installer/data/mysql/mandatory/userflags.sql`, flag 9).

## 3. Paridade

`livre`: edição é interface e modelo interno. O Elzevir difere do Koha de propósito em dois pontos: grava só as partes editadas (o Koha regrava o MARC inteiro a partir do formulário) e recusa edição concorrente pela versão. A paridade exigida continua a da exportação MARC, que o item 13 preserva.

Testes: `crates/elzevir-store/tests/editing.rs` (comandos, versão, exportação, chaves) e `crates/elzevir-app/src/commands.rs` (permissão).

## 4. Fora desta parte

- Editar Hub, relações entre Obras e Itens (2.7, grade de exemplares).
