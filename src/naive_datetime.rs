use core::ffi::{c_char, c_int, c_long, c_void};
use core::{mem, ptr::null_mut as NULL};
use pyo3_ffi::*;

use crate::common::*;
use crate::{
    date::Date,
    date_delta::DateDelta,
    datetime_delta::{set_delta_from_kwarg, DateTimeDelta},
    offset_datetime::{self, OffsetDateTime},
    time::Time,
    time_delta::TimeDelta,
    utc_datetime::Instant,
    zoned_datetime::ZonedDateTime,
    State,
};

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub(crate) struct DateTime {
    pub date: Date,
    pub time: Time,
}

pub(crate) const SINGLETONS: [(&str, DateTime); 2] = [
    (
        "MIN\0",
        DateTime {
            date: Date {
                year: 1,
                month: 1,
                day: 1,
            },
            time: Time {
                hour: 0,
                minute: 0,
                second: 0,
                nanos: 0,
            },
        },
    ),
    (
        "MAX\0",
        DateTime {
            date: Date {
                year: 9999,
                month: 12,
                day: 31,
            },
            time: Time {
                hour: 23,
                minute: 59,
                second: 59,
                nanos: 999_999_999,
            },
        },
    ),
];

impl DateTime {
    #[inline]
    pub(crate) fn default_fmt(&self) -> String {
        if self.time.nanos == 0 {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                self.date.year,
                self.date.month,
                self.date.day,
                self.time.hour,
                self.time.minute,
                self.time.second,
            )
        } else {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}",
                self.date.year,
                self.date.month,
                self.date.day,
                self.time.hour,
                self.time.minute,
                self.time.second,
                self.time.nanos,
            )
            .trim_end_matches('0')
            .to_string()
        }
    }

    #[inline]
    pub(crate) fn shift(self, delta: DateTimeDelta) -> Option<Self> {
        let DateTimeDelta {
            ddelta: DateDelta { months, days },
            tdelta,
        } = delta;
        let DateTime { mut date, time } = self;
        date = date.shift(0, months, days)?;
        let new_time = i128::from(time.total_nanos()) + tdelta.total_nanos();
        let days_delta = new_time.div_euclid(NS_PER_DAY) as i32;
        let nano_delta = new_time.rem_euclid(NS_PER_DAY) as u64;
        if days_delta != 0 {
            date = date.shift_days(days_delta)?;
        }
        Some(DateTime {
            date,
            time: Time::from_total_nanos(nano_delta),
        })
    }

    pub(crate) fn shift_date(self, months: i32, days: i32) -> Option<Self> {
        let DateTime { date, time } = self;
        date.shift(0, months, days)
            .map(|date| DateTime { date, time })
    }

    pub(crate) fn shift_nanos(self, nanos: i128) -> Option<Self> {
        let DateTime { mut date, time } = self;
        let new_time = i128::from(time.total_nanos()) + nanos;
        let days_delta = new_time.div_euclid(NS_PER_DAY) as i32;
        let nano_delta = new_time.rem_euclid(NS_PER_DAY) as u64;
        if days_delta != 0 {
            date = date.shift_days(days_delta)?
        }
        Some(DateTime {
            date,
            time: Time::from_total_nanos(nano_delta),
        })
    }

    pub(crate) fn small_shift_unchecked(self, secs: i32) -> Self {
        debug_assert!(secs.abs() < S_PER_DAY * 2);
        let Self { date, time } = self;
        let day_seconds = time.seconds() + secs;
        let (date, time) = match day_seconds.div_euclid(S_PER_DAY) {
            0 => (date, time.set_seconds(day_seconds as u32)),
            1 => (
                date.increment(),
                time.set_seconds((day_seconds - S_PER_DAY) as u32),
            ),
            -1 => (
                date.decrement(),
                time.set_seconds((day_seconds + S_PER_DAY) as u32),
            ),
            // more than 1 day difference is unlikely--but possible
            2 => (
                date.increment().increment(),
                time.set_seconds((day_seconds - S_PER_DAY * 2) as u32),
            ),
            -2 => (
                date.decrement().decrement(),
                time.set_seconds((day_seconds + S_PER_DAY * 2) as u32),
            ),
            _ => unreachable!(),
        };
        Self { date, time }
    }
}

impl PyWrapped for DateTime {}

unsafe fn __new__(cls: *mut PyTypeObject, args: *mut PyObject, kwargs: *mut PyObject) -> PyReturn {
    let mut year: c_long = 0;
    let mut month: c_long = 0;
    let mut day: c_long = 0;
    let mut hour: c_long = 0;
    let mut minute: c_long = 0;
    let mut second: c_long = 0;
    let mut nanos: c_long = 0;

    // FUTURE: parse them manually, which is more efficient
    if PyArg_ParseTupleAndKeywords(
        args,
        kwargs,
        c"lll|lll$l:NaiveDateTime".as_ptr(),
        vec![
            c"year".as_ptr() as *mut c_char,
            c"month".as_ptr() as *mut c_char,
            c"day".as_ptr() as *mut c_char,
            c"hour".as_ptr() as *mut c_char,
            c"minute".as_ptr() as *mut c_char,
            c"second".as_ptr() as *mut c_char,
            c"nanosecond".as_ptr() as *mut c_char,
            NULL(),
        ]
        .as_mut_ptr(),
        &mut year,
        &mut month,
        &mut day,
        &mut hour,
        &mut minute,
        &mut second,
        &mut nanos,
    ) == 0
    {
        Err(PyErrOccurred())?
    }

    DateTime {
        date: Date::from_longs(year, month, day).ok_or_type_err("Invalid date")?,
        time: Time::from_longs(hour, minute, second, nanos).ok_or_type_err("Invalid time")?,
    }
    .to_obj(cls)
}

unsafe fn __repr__(slf: *mut PyObject) -> PyReturn {
    let DateTime { date, time } = DateTime::extract(slf);
    format!("NaiveDateTime({} {})", date, time).to_py()
}

unsafe fn __str__(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).default_fmt().to_py()
}

unsafe fn format_common_iso(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    __str__(slf)
}

unsafe fn __richcmp__(a_obj: *mut PyObject, b_obj: *mut PyObject, op: c_int) -> PyReturn {
    Ok(newref(if Py_TYPE(b_obj) == Py_TYPE(a_obj) {
        let a = DateTime::extract(a_obj);
        let b = DateTime::extract(b_obj);
        match op {
            pyo3_ffi::Py_LT => a < b,
            pyo3_ffi::Py_LE => a <= b,
            pyo3_ffi::Py_EQ => a == b,
            pyo3_ffi::Py_NE => a != b,
            pyo3_ffi::Py_GT => a > b,
            pyo3_ffi::Py_GE => a >= b,
            _ => unreachable!(),
        }
        .to_py()?
    } else {
        Py_NotImplemented()
    }))
}

unsafe extern "C" fn __hash__(slf: *mut PyObject) -> Py_hash_t {
    let DateTime { date, time } = DateTime::extract(slf);
    hashmask(hash_combine(date.hash() as Py_hash_t, time.pyhash()))
}

unsafe fn __add__(obj_a: *mut PyObject, obj_b: *mut PyObject) -> PyReturn {
    _shift_operator(obj_a, obj_b, false, "+")
}

unsafe fn __sub__(obj_a: *mut PyObject, obj_b: *mut PyObject) -> PyReturn {
    // easy case: subtracting two NaiveDateTime objects
    if Py_TYPE(obj_a) == Py_TYPE(obj_b) {
        let a = DateTime::extract(obj_a);
        let b = DateTime::extract(obj_b);
        TimeDelta::from_nanos_unchecked(
            Instant::from_datetime(a.date, a.time).total_nanos()
                - Instant::from_datetime(b.date, b.time).total_nanos(),
        )
        .to_obj(State::for_type(Py_TYPE(obj_a)).time_delta_type)
    } else {
        _shift_operator(obj_a, obj_b, true, "-")
    }
}

#[inline]
unsafe fn _shift_operator(
    obj_a: *mut PyObject,
    obj_b: *mut PyObject,
    negate: bool,
    opname: &str,
) -> PyReturn {
    let type_b = Py_TYPE(obj_b);
    let type_a = Py_TYPE(obj_a);

    let mod_a = PyType_GetModule(type_a);
    let mod_b = PyType_GetModule(type_b);

    if mod_a == mod_b {
        let mut delta = if type_b == State::for_mod(mod_a).time_delta_type {
            DateTimeDelta {
                ddelta: DateDelta::ZERO,
                tdelta: TimeDelta::extract(obj_b),
            }
        } else if type_b == State::for_mod(mod_a).date_delta_type {
            DateTimeDelta {
                ddelta: DateDelta::extract(obj_b),
                tdelta: TimeDelta::ZERO,
            }
        } else if type_b == State::for_mod(mod_a).datetime_delta_type {
            DateTimeDelta::extract(obj_b)
        } else {
            Err(type_err!(
                "unsupported operand type(s) for {}: 'NaiveDateTime' and {}",
                opname,
                type_b.cast::<PyObject>().repr()
            ))?
        };
        debug_assert_eq!(type_a, State::for_type(type_a).naive_datetime_type);
        let dt = DateTime::extract(obj_a);
        if negate {
            delta = -delta;
        }
        dt.shift(delta)
            .ok_or_else(|| value_err!("Result of {} out of range", opname))?
            .to_obj(type_a)
    } else {
        Ok(newref(Py_NotImplemented()))
    }
}

static mut SLOTS: &[PyType_Slot] = &[
    slotmethod!(Py_tp_new, __new__),
    slotmethod!(Py_tp_repr, __repr__, 1),
    slotmethod!(Py_tp_str, __str__, 1),
    slotmethod!(Py_tp_richcompare, __richcmp__),
    slotmethod!(Py_nb_add, __add__, 2),
    slotmethod!(Py_nb_subtract, __sub__, 2),
    PyType_Slot {
        slot: Py_tp_hash,
        pfunc: __hash__ as *mut c_void,
    },
    PyType_Slot {
        slot: Py_tp_methods,
        pfunc: unsafe { METHODS.as_ptr() as *mut c_void },
    },
    PyType_Slot {
        slot: Py_tp_getset,
        pfunc: unsafe { GETSETTERS.as_ptr() as *mut c_void },
    },
    PyType_Slot {
        slot: Py_tp_dealloc,
        pfunc: generic_dealloc as *mut c_void,
    },
    PyType_Slot {
        slot: 0,
        pfunc: NULL(),
    },
];

unsafe fn replace(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
) -> PyReturn {
    if !args.is_empty() {
        Err(type_err!("replace() takes no positional arguments"))?
    }
    let module = State::for_type(cls);
    let dt = DateTime::extract(slf);
    let mut year = dt.date.year as c_long;
    let mut month = dt.date.month as c_long;
    let mut day = dt.date.day as c_long;
    let mut hour = dt.time.hour as c_long;
    let mut minute = dt.time.minute as c_long;
    let mut second = dt.time.second as c_long;
    let mut nanos = dt.time.nanos as c_long;
    for &(name, value) in kwargs {
        if name == module.str_year {
            year = value.to_long()?.ok_or_type_err("year must be an integer")?;
        } else if name == module.str_month {
            month = value
                .to_long()?
                .ok_or_type_err("month must be an integer")?;
        } else if name == module.str_day {
            day = value.to_long()?.ok_or_type_err("day must be an integer")?;
        } else if name == module.str_hour {
            hour = value.to_long()?.ok_or_type_err("hour must be an integer")?;
        } else if name == module.str_minute {
            minute = value
                .to_long()?
                .ok_or_type_err("minute must be an integer")?;
        } else if name == module.str_second {
            second = value
                .to_long()?
                .ok_or_type_err("second must be an integer")?;
        } else if name == module.str_nanosecond {
            nanos = value
                .to_long()?
                .ok_or_type_err("nanosecond must be an integer")?;
        } else {
            Err(type_err!(
                "replace() got an unexpected keyword argument: {}",
                name.repr()
            ))?
        }
    }
    DateTime {
        date: Date::from_longs(year, month, day).ok_or_value_err("Invalid date")?,
        time: Time::from_longs(hour, minute, second, nanos).ok_or_value_err("Invalid time")?,
    }
    .to_obj(cls)
}

unsafe fn add(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
) -> PyReturn {
    _shift_method(slf, cls, args, kwargs, false, "add")
}

unsafe fn subtract(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
) -> PyReturn {
    _shift_method(slf, cls, args, kwargs, true, "subtract")
}

#[inline]
unsafe fn _shift_method(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
    negate: bool,
    fname: &str,
) -> PyReturn {
    if !args.is_empty() {
        Err(type_err!("{}() takes no positional arguments", fname))?
    }
    let state = State::for_type(cls);
    let mut months = 0;
    let mut days = 0;
    let mut nanos = 0;
    for &(name, value) in kwargs {
        set_delta_from_kwarg(
            name,
            value,
            &mut months,
            &mut days,
            &mut nanos,
            state,
            fname,
        )?;
    }
    if negate {
        months = -months;
        days = -days;
        nanos = -nanos;
    }
    DateTime::extract(slf)
        .shift_date(months, days)
        .and_then(|dt| dt.shift_nanos(nanos))
        .ok_or_else(|| value_err!("Result of {}() out of range", fname))?
        .to_obj(cls)
}

unsafe fn __reduce__(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    let DateTime {
        date: Date { year, month, day },
        time:
            Time {
                hour,
                minute,
                second,
                nanos,
            },
    } = DateTime::extract(slf);
    PyTuple_Pack(
        2,
        State::for_obj(slf).unpickle_naive_datetime,
        steal!(PyTuple_Pack(
            1,
            pack![year, month, day, hour, minute, second, nanos].to_py()?
        )
        .as_result()?),
    )
    .as_result()
}

pub(crate) unsafe fn unpickle(module: *mut PyObject, arg: *mut PyObject) -> PyReturn {
    let mut packed = arg.to_bytes()?.ok_or_type_err("Invalid pickle data")?;
    if packed.len() != 11 {
        Err(type_err!("Invalid pickle data"))?
    }
    DateTime {
        date: Date {
            year: unpack_one!(packed, u16),
            month: unpack_one!(packed, u8),
            day: unpack_one!(packed, u8),
        },
        time: Time {
            hour: unpack_one!(packed, u8),
            minute: unpack_one!(packed, u8),
            second: unpack_one!(packed, u8),
            nanos: unpack_one!(packed, u32),
        },
    }
    .to_obj(State::for_mod(module).naive_datetime_type)
}

unsafe fn from_py_datetime(type_: *mut PyObject, dt: *mut PyObject) -> PyReturn {
    if PyDateTime_Check(dt) == 0 {
        Err(type_err!("argument must be datetime.datetime"))?
    }
    let tzinfo = get_dt_tzinfo(dt);
    if tzinfo != Py_None() {
        Err(value_err!(
            "datetime must be naive, but got tzinfo={}",
            tzinfo.repr()
        ))?
    }
    DateTime {
        date: Date {
            year: PyDateTime_GET_YEAR(dt) as u16,
            month: PyDateTime_GET_MONTH(dt) as u8,
            day: PyDateTime_GET_DAY(dt) as u8,
        },
        time: Time {
            hour: PyDateTime_DATE_GET_HOUR(dt) as u8,
            minute: PyDateTime_DATE_GET_MINUTE(dt) as u8,
            second: PyDateTime_DATE_GET_SECOND(dt) as u8,
            nanos: PyDateTime_DATE_GET_MICROSECOND(dt) as u32 * 1_000,
        },
    }
    .to_obj(type_.cast())
}

unsafe fn py_datetime(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    let DateTime {
        date: Date { year, month, day },
        time:
            Time {
                hour,
                minute,
                second,
                nanos,
            },
    } = DateTime::extract(slf);
    let &PyDateTime_CAPI {
        DateTime_FromDateAndTime,
        DateTimeType,
        ..
    } = State::for_type(Py_TYPE(slf)).py_api;
    DateTime_FromDateAndTime(
        year.into(),
        month.into(),
        day.into(),
        hour.into(),
        minute.into(),
        second.into(),
        (nanos / 1_000) as c_int,
        Py_None(),
        DateTimeType,
    )
    .as_result()
}

unsafe fn get_date(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    DateTime::extract(slf)
        .date
        .to_obj(State::for_obj(slf).date_type)
}

unsafe fn get_time(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    DateTime::extract(slf)
        .time
        .to_obj(State::for_obj(slf).time_type)
}

pub fn parse_date_and_time(s: &[u8]) -> Option<(Date, Time)> {
    // This should have already been checked by caller
    debug_assert!(
        s.len() >= 19 && (s[10] == b' ' || s[10] == b'T' || s[10] == b't' || s[10] == b'_')
    );
    Date::parse_all(&s[..10]).zip(Time::parse_all(&s[11..]))
}

unsafe fn parse_common_iso(cls: *mut PyObject, arg: *mut PyObject) -> PyReturn {
    let s = arg.to_utf8()?.ok_or_type_err("Expected a string")?;
    if s.len() < 19 || s[10] != b'T' {
        Err(value_err!("Invalid format: {}", arg.repr()))
    } else {
        match parse_date_and_time(s) {
            Some((date, time)) => DateTime { date, time }.to_obj(cls.cast()),
            None => Err(value_err!("Invalid format: {}", arg.repr())),
        }
    }
}

unsafe fn strptime(cls: *mut PyObject, args: &[*mut PyObject]) -> PyReturn {
    if args.len() != 2 {
        type_err!(
            "strptime() takes exactly 2 arguments ({} given)",
            args.len()
        )
        .err()?
    }
    // OPTIMIZE: get this working with vectorcall
    let parsed = PyObject_Call(
        State::for_type(cls.cast()).strptime,
        steal!(PyTuple_Pack(2, args[0], args[1]).as_result()?),
        NULL(),
    )
    .as_result()?;
    defer_decref!(parsed);
    let tzinfo = get_dt_tzinfo(parsed);
    if tzinfo != Py_None() {
        Err(value_err!(
            "datetime must be naive, but got tzinfo={}",
            tzinfo.repr()
        ))?;
    }
    DateTime {
        date: Date {
            year: PyDateTime_GET_YEAR(parsed) as u16,
            month: PyDateTime_GET_MONTH(parsed) as u8,
            day: PyDateTime_GET_DAY(parsed) as u8,
        },
        time: Time {
            hour: PyDateTime_DATE_GET_HOUR(parsed) as u8,
            minute: PyDateTime_DATE_GET_MINUTE(parsed) as u8,
            second: PyDateTime_DATE_GET_SECOND(parsed) as u8,
            nanos: PyDateTime_DATE_GET_MICROSECOND(parsed) as u32 * 1_000,
        },
    }
    .to_obj(cls.cast())
}

unsafe fn assume_utc(slf: *mut PyObject, _: *mut PyObject) -> PyReturn {
    let DateTime { date, time } = DateTime::extract(slf);
    Instant::from_datetime(date, time).to_obj(State::for_obj(slf).utc_datetime_type)
}

unsafe fn assume_fixed_offset(slf: *mut PyObject, arg: *mut PyObject) -> PyReturn {
    let &State {
        time_delta_type,
        offset_datetime_type,
        ..
    } = State::for_obj(slf);
    DateTime::extract(slf)
        .with_offset(offset_datetime::extract_offset(arg, time_delta_type)?)
        .ok_or_value_err("Datetime out of range")?
        .to_obj(offset_datetime_type)
}

unsafe fn assume_tz(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
) -> PyReturn {
    let &State {
        py_api,
        zoneinfo_type,
        str_disambiguate,
        zoned_datetime_type,
        exc_skipped,
        exc_ambiguous,
        ..
    } = State::for_type(cls);
    let DateTime { date, time } = DateTime::extract(slf);
    if args.len() != 1 {
        type_err!(
            "assume_tz() takes 1 positional argument but {} were given",
            args.len()
        )
        .err()?
    }

    let dis = Disambiguate::from_only_kwarg(kwargs, str_disambiguate, "assume_tz")?;
    let zoneinfo = call1(zoneinfo_type, args[0])?;
    defer_decref!(zoneinfo);
    ZonedDateTime::from_naive(py_api, date, time, zoneinfo, dis)?
        .map_err(|e| match e {
            Ambiguity::Fold => py_err!(
                exc_ambiguous,
                "{} {} is ambiguous in the timezone {}",
                date,
                time,
                args[0].repr()
            ),
            Ambiguity::Gap => py_err!(
                exc_skipped,
                "{} {} is skipped in the timezone {}",
                date,
                time,
                args[0].repr()
            ),
        })?
        .to_obj(zoned_datetime_type)
}

unsafe fn assume_local_system(
    slf: *mut PyObject,
    cls: *mut PyTypeObject,
    args: &[*mut PyObject],
    kwargs: &[(*mut PyObject, *mut PyObject)],
) -> PyReturn {
    let &State {
        py_api,
        str_disambiguate,
        local_datetime_type,
        exc_skipped,
        exc_ambiguous,
        ..
    } = State::for_type(cls);
    let DateTime { date, time } = DateTime::extract(slf);
    if !args.is_empty() {
        Err(type_err!(
            "assume_local_system() takes no positional arguments"
        ))?
    }

    let dis = Disambiguate::from_only_kwarg(kwargs, str_disambiguate, "assume_local_system")?;
    OffsetDateTime::for_localsystem(py_api, date, time, dis)?
        .map_err(|e| match e {
            Ambiguity::Fold => py_err!(
                exc_ambiguous,
                "{} {} is ambiguous in the system timezone",
                date,
                time,
            ),
            Ambiguity::Gap => py_err!(
                exc_skipped,
                "{} {} is skipped in the system timezone",
                date,
                time,
            ),
        })?
        .to_obj(local_datetime_type)
}

unsafe fn replace_date(slf: *mut PyObject, arg: *mut PyObject) -> PyReturn {
    let cls = Py_TYPE(slf);
    let DateTime { time, .. } = DateTime::extract(slf);
    if Py_TYPE(arg) == State::for_type(cls).date_type {
        DateTime {
            date: Date::extract(arg),
            time,
        }
        .to_obj(cls)
    } else {
        Err(type_err!("date must be a Date instance"))
    }
}

unsafe fn replace_time(slf: *mut PyObject, arg: *mut PyObject) -> PyReturn {
    let cls = Py_TYPE(slf);
    let DateTime { date, .. } = DateTime::extract(slf);
    if Py_TYPE(arg) == State::for_type(cls).time_type {
        DateTime {
            date,
            time: Time::extract(arg),
        }
        .to_obj(cls)
    } else {
        Err(type_err!("time must be a Time instance"))
    }
}

static mut METHODS: &[PyMethodDef] = &[
    method!(identity2 named "__copy__", ""),
    method!(identity2 named "__deepcopy__", "", METH_O),
    method!(
        from_py_datetime,
        "Create an instance from a datetime.datetime",
        METH_CLASS | METH_O
    ),
    method!(py_datetime, "Convert to a datetime.datetime"),
    method!(
        get_date named "date",
        "Get the date component"
    ),
    method!(
        get_time named "time",
        "Get the time component"
    ),
    method!(
        format_common_iso,
        "Get the common ISO 8601 string representation"
    ),
    method!(
        parse_common_iso,
        "Create an instance from the common ISO 8601 string representation",
        METH_O | METH_CLASS
    ),
    method!(__reduce__, ""),
    method_vararg!(strptime, "Parse a string into a NaiveDateTime", METH_CLASS),
    method_kwargs!(
        replace,
        "Return a new instance with the specified fields replaced"
    ),
    method!(assume_utc, "Assume the datetime is in UTC"),
    method!(
        assume_fixed_offset,
        "Assume the datetime has a fixed offset",
        METH_O
    ),
    method_kwargs!(assume_tz, "Assume the datetime is in a timezone"),
    method_kwargs!(
        assume_local_system,
        "Assume the datetime is in the local system timezone"
    ),
    method!(
        replace_date,
        "Return a new instance with the date replaced",
        METH_O
    ),
    method!(
        replace_time,
        "Return a new instance with the time replaced",
        METH_O
    ),
    method_kwargs!(add, "Add various time and/or calendar units"),
    method_kwargs!(subtract, "Subtract various time and/or calendar units"),
    PyMethodDef::zeroed(),
];

unsafe fn get_year(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).date.year.to_py()
}

unsafe fn get_month(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).date.month.to_py()
}

unsafe fn get_day(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).date.day.to_py()
}

unsafe fn get_hour(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).time.hour.to_py()
}

unsafe fn get_minute(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).time.minute.to_py()
}

unsafe fn get_second(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).time.second.to_py()
}

unsafe fn get_nanos(slf: *mut PyObject) -> PyReturn {
    DateTime::extract(slf).time.nanos.to_py()
}

static mut GETSETTERS: &[PyGetSetDef] = &[
    getter!(
        get_year named "year",
        "The year component"
    ),
    getter!(
        get_month named "month",
        "The month component"
    ),
    getter!(
        get_day named "day",
        "The day component"
    ),
    getter!(
        get_hour named "hour",
        "The hour component"
    ),
    getter!(
        get_minute named "minute",
        "The minute component"
    ),
    getter!(
        get_second named "second",
        "The second component"
    ),
    getter!(
        get_nanos named "nanosecond",
        "The nanosecond component"
    ),
    PyGetSetDef {
        name: NULL(),
        get: None,
        set: None,
        doc: NULL(),
        closure: NULL(),
    },
];

type NaiveDateTime = DateTime;
type_spec!(NaiveDateTime, SLOTS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        assert_eq!(
            parse_date_and_time(b"2023-03-02 02:09:09"),
            Some((
                Date {
                    year: 2023,
                    month: 3,
                    day: 2,
                },
                Time {
                    hour: 2,
                    minute: 9,
                    second: 9,
                    nanos: 0,
                },
            ))
        );
        assert_eq!(
            parse_date_and_time(b"2023-03-02 02:09:09.123456789"),
            Some((
                Date {
                    year: 2023,
                    month: 3,
                    day: 2,
                },
                Time {
                    hour: 2,
                    minute: 9,
                    second: 9,
                    nanos: 123_456_789,
                },
            ))
        );
    }

    #[test]
    fn test_parse_invalid() {
        // dot but no fractional digits
        assert_eq!(parse_date_and_time(b"2023-03-02 02:09:09."), None);
        // too many fractions
        assert_eq!(parse_date_and_time(b"2023-03-02 02:09:09.1234567890"), None);
        // invalid minute
        assert_eq!(parse_date_and_time(b"2023-03-02 02:69:09.123456789"), None);
        // invalid date
        assert_eq!(parse_date_and_time(b"2023-02-29 02:29:09.123456789"), None);
    }
}