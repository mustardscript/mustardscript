use super::*;

impl Runtime {
    fn promise_receiver(&self, value: Value, method: &str) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Promise.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn collect_iterable_values(&mut self, iterable: Value) -> JsliteResult<Vec<Value>> {
        let iterator = self.create_iterator(iterable)?;
        let mut values = Vec::new();
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            values.push(value);
        }
        Ok(values)
    }

    pub(crate) fn call_promise_then(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "then")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_fulfilled = args.first().cloned().filter(is_callable);
        let on_rejected = args.get(1).cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_catch(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "catch")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_rejected = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled: None,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_finally(
        &mut self,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "finally")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let callback = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(promise, PromiseReaction::Finally { target, callback })?;
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_all(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::All {
            remaining: values.len(),
            values: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::All,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_race(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        for value in
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?
        {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index: 0,
                    kind: PromiseCombinatorKind::Race,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_any(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let error = self.make_aggregate_error(Vec::new())?;
            self.reject_promise(
                target,
                PromiseRejection {
                    value: error,
                    span: None,
                    traceback: self.traceback_snapshots(),
                },
            )?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::Any {
            remaining: values.len(),
            reasons: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::Any,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    pub(crate) fn call_promise_all_settled(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::AllSettled {
            remaining: values.len(),
            results: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::AllSettled,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }
}
