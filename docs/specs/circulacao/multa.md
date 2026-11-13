# Multa por atraso

> Status: tarefa 1.12. Base: Koha 26.05.05. Cobre o cálculo da multa de um empréstimo; lançar a dívida na conta do leitor, multa diária pelo cron e suspensão (`finedays`) ficam para a Fase 3.

## 1. Comportamento

1. Sem atraso (devolução no vencimento ou antes) não há multa: valor zero, zero unidades.
2. As unidades em atraso seguem o `lengthunit` da regra (padrão `days`):
   - **dias**: diferença de datas do calendário entre o vencimento e a devolução, sem olhar a hora;
   - **horas**: horas inteiras de relógio entre os dois instantes. Com menos de uma hora de atraso, conta 1 se sobram segundos além dos minutos inteiros, e 0 se o atraso é um número exato de minutos.
3. Com `finesCalendar = noFinesWhenClosed`, cada dia fechado entre o vencimento e a devolução (inclusive os dois) tira um dia, ou 24 horas, das unidades. Com `ignoreCalendar` todos os dias contam.
4. A carência é `firstremind` (em unidades). Só há cobrança se `chargeperiod` é maior que zero e as unidades passam da carência.
5. As unidades cobradas são as que passam da carência; com `FinesIncludeGracePeriod`, são todas.
6. Os períodos são unidades ÷ `chargeperiod`, arredondado para baixo; para cima com `chargeperiod_charge_at = 1` (cobra no início de cada período).
7. O valor é períodos × `fine`. Se o registro tem uma reserva de *recall* em atraso, o valor é períodos × `recall_overdue_fine`, e zero se a regra não define esse valor (o Koha não volta para `fine`).
8. `overduefinescap` limita o valor quando é maior que zero (`0` e `0.00` não limitam).
9. Com `useDefaultReplacementCost`, um exemplar sem preço de reposição (ou com zero) usa o custo padrão do tipo de item.
10. Com `cap_fine_to_replacement_price`, o valor fica limitado ao preço de reposição quando ele não é zero.
11. Dinheiro é inteiro em centavos; não há arredondamento de ponto flutuante.

## 2. Origem no Koha

- `C4::Overdues::CalcFine` e `get_chargeable_units` (`reference/koha/C4/Overdues.pm`).
- `Koha::Calendar::days_between` e `hours_between`.
- Teste: `reference/koha/t/db_dependent/Circulation/CalcFine.t`.

## 3. Paridade

`exigida`. Cenários:

- `scenarios/circulacao/calcfine.t.yaml`: as 10 asserções de `CalcFine.t` (formato no doc 05, §4), pela devolução com `CalculateFinesOnReturn`.

## 4. Casos de borda

1. Devolução no próprio vencimento: zero.
2. 29 dias de atraso a 1,00 por dia: 29,00; com `overduefinescap` `0` ou `0.00`, o mesmo.
3. `fine` vazio: zero.
4. Carência de 2 dias e 5 de atraso: 3 unidades cobradas; com `FinesIncludeGracePeriod`, 5.
5. `chargeperiod` 7 e 10 dias de atraso: 1 período; com cobrança no início, 2.
6. Teto de 3,00 e reposição de 5,00: 3,00; teto de 6,00: 5,00.
7. Preço de reposição zero e custo padrão 6,00: o limite é 6,00.
8. *Recall* em atraso com `recall_overdue_fine` 5,00 e 5 dias: 25,00; sem *recall*, 5,00.
9. Empréstimo por hora devolvido 30 minutos depois: 0 horas; 2h30 depois: 2 horas.

## 5. Desvios

Nenhum. O zero com *recall* sem `recall_overdue_fine` e a regra dos segundos do item 2 são do Koha e estão reproduzidos.

## 6. Fora do escopo

- Plugins `overwrite_calc_fine`.
- Lançamento da multa na conta (`UpdateFine`), multa acumulada pelo cron (`fines.pl`), suspensão por atraso (`finedays`).
- Distinguir *recall* do exemplar e *recall* do registro: hoje basta "o registro tem um *recall* em atraso", que dá o mesmo valor nos dois casos.
