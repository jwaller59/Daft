//! Defines the arithmetic kernels for adding a Duration to a Timestamp,
//! Time32, Time64, Date32 and Date64.
//!
//! For the purposes of Arrow Implementations, adding this value to a Timestamp
//! ("t1") naively (i.e. simply summing the two number) is acceptable even
//! though in some cases the resulting Timestamp (t2) would not account for
//! leap-seconds during the elapsed time between "t1" and "t2".  Similarly,
//! representing the difference between two Unix timestamp is acceptable, but
//! would yield a value that is possibly a few seconds off from the true
//! elapsed time.

use std::ops::{Add, Sub};

use num_traits::AsPrimitive;

use crate::{
    array::PrimitiveArray,
    compute::arity::{binary, unary},
    datatypes::{DataType, TimeUnit},
    error::{Error, Result},
    scalar::{PrimitiveScalar, Scalar},
    temporal_conversions,
    types::{months_days_ns, NativeType},
};

/// Creates the scale required to add or subtract a Duration to a time array
/// (Timestamp, Time, or Date). The resulting scale always multiplies the rhs
/// number (Duration) so it can be added to the lhs number (time array).
fn create_scale(lhs: &DataType, rhs: &DataType) -> Result<f64> {
    // Matching on both data types from both numbers to calculate the correct
    // scale for the operation. The timestamp, Time and duration have a
    // Timeunit enum in its data type. This enum is used to describe the
    // addition of the duration. The Date32 and Date64 have different rules for
    // the scaling.
    let scale = match (lhs, rhs) {
        (DataType::Timestamp(timeunit_a, _), DataType::Duration(timeunit_b))
        | (DataType::Time32(timeunit_a), DataType::Duration(timeunit_b))
        | (DataType::Time64(timeunit_a), DataType::Duration(timeunit_b)) => {
            // The scale is based on the TimeUnit that each of the numbers have.
            temporal_conversions::timeunit_scale(*timeunit_a, *timeunit_b)
        }
        (DataType::Date32, DataType::Duration(timeunit)) => {
            // Date32 represents the time elapsed time since UNIX epoch
            // (1970-01-01) in days (32 bits). The duration value has to be
            // scaled to days to be able to add the value to the Date.
            temporal_conversions::timeunit_scale(TimeUnit::Second, *timeunit)
                / temporal_conversions::SECONDS_IN_DAY as f64
        }
        (DataType::Date64, DataType::Duration(timeunit)) => {
            // Date64 represents the time elapsed time since UNIX epoch
            // (1970-01-01) in milliseconds (64 bits). The duration value has
            // to be scaled to milliseconds to be able to add the value to the
            // Date.
            temporal_conversions::timeunit_scale(TimeUnit::Millisecond, *timeunit)
        }
        _ => {
            return Err(Error::InvalidArgumentError(
                "Incorrect data type for the arguments".to_string(),
            ));
        }
    };

    Ok(scale)
}

/// Adds a duration to a time array (Timestamp, Time and Date). The timeunit
/// enum is used to scale correctly both arrays; adding seconds with seconds,
/// or milliseconds with milliseconds.
///
/// # Examples
/// ```rust,ignore
/// use arrow2::compute::arithmetics::time::add_duration;
/// use arrow2::array::PrimitiveArray;
/// use arrow2::datatypes::{DataType, TimeUnit};
///
/// let timestamp = PrimitiveArray::from([
///     Some(100000i64),
///     Some(200000i64),
///     None,
///     Some(300000i64),
/// ])
/// .to(DataType::Timestamp(
///     TimeUnit::Second,
///     Some("America/New_York".to_string()),
/// ));
///
/// let duration = PrimitiveArray::from([Some(10i64), Some(20i64), None, Some(30i64)])
///     .to(DataType::Duration(TimeUnit::Second));
///
/// let result = add_duration(&timestamp, &duration);
/// let expected = PrimitiveArray::from([
///     Some(100010i64),
///     Some(200020i64),
///     None,
///     Some(300030i64),
/// ])
/// .to(DataType::Timestamp(
///     TimeUnit::Second,
///     Some("America/New_York".to_string()),
/// ));
///
/// assert_eq!(result, expected);
/// ```
pub fn add_duration<T>(
    time: &PrimitiveArray<T>,
    duration: &PrimitiveArray<i64>,
) -> PrimitiveArray<T>
where
    f64: AsPrimitive<T>,
    T: NativeType + Add<T, Output = T>,
{
    let scale = create_scale(time.data_type(), duration.data_type()).unwrap();

    // Closure for the binary operation. The closure contains the scale
    // required to add a duration to the timestamp array.
    let op = move |a: T, b: i64| a + (b as f64 * scale).as_();

    binary(time, duration, time.data_type().clone(), op)
}

/// Adds a duration to a time array (Timestamp, Time and Date). The timeunit
/// enum is used to scale correctly both arrays; adding seconds with seconds,
/// or milliseconds with milliseconds.
pub fn add_duration_scalar<T>(
    time: &PrimitiveArray<T>,
    duration: &PrimitiveScalar<i64>,
) -> PrimitiveArray<T>
where
    f64: AsPrimitive<T>,
    T: NativeType + Add<T, Output = T>,
{
    let scale = create_scale(time.data_type(), duration.data_type()).unwrap();
    let duration = if let Some(duration) = *duration.value() {
        duration
    } else {
        return PrimitiveArray::<T>::new_null(time.data_type().clone(), time.len());
    };

    // Closure for the binary operation. The closure contains the scale
    // required to add a duration to the timestamp array.
    let op = move |a: T| a + (duration as f64 * scale).as_();

    unary(time, op, time.data_type().clone())
}

/// Subtract a duration to a time array (Timestamp, Time and Date). The timeunit
/// enum is used to scale correctly both arrays; adding seconds with seconds,
/// or milliseconds with milliseconds.
///
/// # Examples
/// ```rust,ignore
/// use arrow2::compute::arithmetics::time::subtract_duration;
/// use arrow2::array::PrimitiveArray;
/// use arrow2::datatypes::{DataType, TimeUnit};
///
/// let timestamp = PrimitiveArray::from([
///     Some(100000i64),
///     Some(200000i64),
///     None,
///     Some(300000i64),
/// ])
/// .to(DataType::Timestamp(
///     TimeUnit::Second,
///     Some("America/New_York".to_string()),
/// ));
///
/// let duration = PrimitiveArray::from([Some(10i64), Some(20i64), None, Some(30i64)])
///     .to(DataType::Duration(TimeUnit::Second));
///
/// let result = subtract_duration(&timestamp, &duration);
/// let expected = PrimitiveArray::from([
///     Some(99990i64),
///     Some(199980i64),
///     None,
///     Some(299970i64),
/// ])
/// .to(DataType::Timestamp(
///     TimeUnit::Second,
///     Some("America/New_York".to_string()),
/// ));
///
/// assert_eq!(result, expected);
///
/// ```
pub fn subtract_duration<T>(
    time: &PrimitiveArray<T>,
    duration: &PrimitiveArray<i64>,
) -> PrimitiveArray<T>
where
    f64: AsPrimitive<T>,
    T: NativeType + Sub<T, Output = T>,
{
    let scale = create_scale(time.data_type(), duration.data_type()).unwrap();

    // Closure for the binary operation. The closure contains the scale
    // required to add a duration to the timestamp array.
    let op = move |a: T, b: i64| a - (b as f64 * scale).as_();

    binary(time, duration, time.data_type().clone(), op)
}

/// Subtract a duration to a time array (Timestamp, Time and Date). The timeunit
/// enum is used to scale correctly both arrays; adding seconds with seconds,
/// or milliseconds with milliseconds.
pub fn sub_duration_scalar<T>(
    time: &PrimitiveArray<T>,
    duration: &PrimitiveScalar<i64>,
) -> PrimitiveArray<T>
where
    f64: AsPrimitive<T>,
    T: NativeType + Sub<T, Output = T>,
{
    let scale = create_scale(time.data_type(), duration.data_type()).unwrap();
    let duration = if let Some(duration) = *duration.value() {
        duration
    } else {
        return PrimitiveArray::<T>::new_null(time.data_type().clone(), time.len());
    };

    let op = move |a: T| a - (duration as f64 * scale).as_();

    unary(time, op, time.data_type().clone())
}

/// Calculates the difference between two timestamps returning an array of type
/// Duration. The timeunit enum is used to scale correctly both arrays;
/// subtracting seconds with seconds, or milliseconds with milliseconds.
///
/// # Examples
/// ```rust,ignore
/// use arrow2::compute::arithmetics::time::subtract_timestamps;
/// use arrow2::array::PrimitiveArray;
/// use arrow2::datatypes::{DataType, TimeUnit};
/// let timestamp_a = PrimitiveArray::from([
///     Some(100_010i64),
///     Some(200_020i64),
///     None,
///     Some(300_030i64),
/// ])
/// .to(DataType::Timestamp(TimeUnit::Second, None));
///
/// let timestamp_b = PrimitiveArray::from([
///     Some(100_000i64),
///     Some(200_000i64),
///     None,
///     Some(300_000i64),
/// ])
/// .to(DataType::Timestamp(TimeUnit::Second, None));
///
/// let expected = PrimitiveArray::from([Some(10i64), Some(20i64), None, Some(30i64)])
///     .to(DataType::Duration(TimeUnit::Second));
///
/// let result = subtract_timestamps(&timestamp_a, &&timestamp_b).unwrap();
/// assert_eq!(result, expected);
/// ```
pub fn subtract_timestamps(
    lhs: &PrimitiveArray<i64>,
    rhs: &PrimitiveArray<i64>,
) -> Result<PrimitiveArray<i64>> {
    // Matching on both data types from both arrays.
    // Both timestamps have a Timeunit enum in its data type.
    // This enum is used to adjust the scale between the timestamps.
    match (lhs.data_type(), rhs.data_type()) {
        // Naive timestamp comparison. It doesn't take into account timezones
        // from the Timestamp timeunit.
        (DataType::Timestamp(timeunit_a, None), DataType::Timestamp(timeunit_b, None)) => {
            // Closure for the binary operation. The closure contains the scale
            // required to calculate the difference between the timestamps.
            let scale = temporal_conversions::timeunit_scale(*timeunit_a, *timeunit_b);
            let op = move |a, b| a - (b as f64 * scale) as i64;

            Ok(binary(lhs, rhs, DataType::Duration(*timeunit_a), op))
        }
        _ => Err(Error::InvalidArgumentError(
            "Incorrect data type for the arguments".to_string(),
        )),
    }
}

/// Calculates the difference between two timestamps as [`DataType::Duration`] with the same time scale.
pub fn sub_timestamps_scalar(
    lhs: &PrimitiveArray<i64>,
    rhs: &PrimitiveScalar<i64>,
) -> Result<PrimitiveArray<i64>> {
    let (scale, timeunit_a) =
        if let (DataType::Timestamp(timeunit_a, None), DataType::Timestamp(timeunit_b, None)) =
            (lhs.data_type(), rhs.data_type())
        {
            (
                temporal_conversions::timeunit_scale(*timeunit_a, *timeunit_b),
                timeunit_a,
            )
        } else {
            return Err(Error::InvalidArgumentError(
                "sub_timestamps_scalar requires both arguments to be timestamps without timezone"
                    .to_string(),
            ));
        };

    let rhs = if let Some(value) = *rhs.value() {
        value
    } else {
        return Ok(PrimitiveArray::<i64>::new_null(
            lhs.data_type().clone(),
            lhs.len(),
        ));
    };

    let op = move |a| a - (rhs as f64 * scale) as i64;

    Ok(unary(lhs, op, DataType::Duration(*timeunit_a)))
}

/// Adds an interval to a [`DataType::Timestamp`].
pub fn add_interval(
    timestamp: &PrimitiveArray<i64>,
    interval: &PrimitiveArray<months_days_ns>,
) -> Result<PrimitiveArray<i64>> {
    if interval.len() == 1 {
        let value = interval.get(0);
        let dtype = interval.data_type().clone();
        let scalar = PrimitiveScalar::new(dtype, value);
        return add_interval_scalar(timestamp, &scalar);
    }
    if timestamp.len() != interval.len() {
        return Err(Error::InvalidArgumentError(
            "Timestamp and interval arrays must have the same length".to_string(),
        ));
    }
    match timestamp.data_type().to_logical_type() {
        DataType::Timestamp(time_unit, Some(timezone_str)) => {
            let time_unit = *time_unit;
            let timezone = temporal_conversions::parse_offset(timezone_str);
            match timezone {
                Ok(timezone) => Ok(binary(
                    timestamp,
                    interval,
                    timestamp.data_type().clone(),
                    |timestamp, interval| {
                        temporal_conversions::add_interval(
                            timestamp, time_unit, interval, &timezone,
                        )
                    },
                )),
                #[cfg(feature = "chrono-tz")]
                Err(_) => {
                    let timezone = temporal_conversions::parse_offset_tz(timezone_str)?;
                    Ok(binary(
                        timestamp,
                        interval,
                        timestamp.data_type().clone(),
                        |timestamp, interval| {
                            temporal_conversions::add_interval(
                                timestamp, time_unit, interval, &timezone,
                            )
                        },
                    ))
                }
                #[cfg(not(feature = "chrono-tz"))]
                _ => Err(Error::InvalidArgumentError(format!(
                    "timezone \"{}\" cannot be parsed (feature chrono-tz is not active)",
                    timezone_str
                ))),
            }
        }
        DataType::Timestamp(time_unit, None) => {
            let time_unit = *time_unit;
            Ok(binary(
                timestamp,
                interval,
                timestamp.data_type().clone(),
                |timestamp, interval| {
                    temporal_conversions::add_naive_interval(timestamp, time_unit, interval)
                },
            ))
        }
        _ => Err(Error::InvalidArgumentError(
            "Adding an interval is only supported for `DataType::Timestamp`".to_string(),
        )),
    }
}

/// Adds an interval to a [`DataType::Timestamp`].
pub fn add_interval_scalar(
    timestamp: &PrimitiveArray<i64>,
    interval: &PrimitiveScalar<months_days_ns>,
) -> Result<PrimitiveArray<i64>> {
    let interval = if let Some(interval) = *interval.value() {
        interval
    } else {
        return Ok(PrimitiveArray::<i64>::new_null(
            timestamp.data_type().clone(),
            timestamp.len(),
        ));
    };

    match timestamp.data_type().to_logical_type() {
        DataType::Timestamp(time_unit, Some(timezone_str)) => {
            let time_unit = *time_unit;
            let timezone = temporal_conversions::parse_offset(timezone_str);
            match timezone {
                Ok(timezone) => Ok(unary(
                    timestamp,
                    |timestamp| {
                        temporal_conversions::add_interval(
                            timestamp, time_unit, interval, &timezone,
                        )
                    },
                    timestamp.data_type().clone(),
                )),
                #[cfg(feature = "chrono-tz")]
                Err(_) => {
                    let timezone = temporal_conversions::parse_offset_tz(timezone_str)?;
                    Ok(unary(
                        timestamp,
                        |timestamp| {
                            temporal_conversions::add_interval(
                                timestamp, time_unit, interval, &timezone,
                            )
                        },
                        timestamp.data_type().clone(),
                    ))
                }
                #[cfg(not(feature = "chrono-tz"))]
                _ => Err(Error::InvalidArgumentError(format!(
                    "timezone \"{}\" cannot be parsed (feature chrono-tz is not active)",
                    timezone_str
                ))),
            }
        }
        DataType::Timestamp(time_unit, None) => {
            let time_unit = *time_unit;
            Ok(unary(
                timestamp,
                |timestamp| {
                    temporal_conversions::add_naive_interval(timestamp, time_unit, interval)
                },
                timestamp.data_type().clone(),
            ))
        }
        _ => Err(Error::InvalidArgumentError(
            "Adding an interval is only supported for `DataType::Timestamp`".to_string(),
        )),
    }
}

pub fn sub_interval(
    timestamp: &PrimitiveArray<i64>,
    interval: &PrimitiveArray<months_days_ns>,
) -> Result<PrimitiveArray<i64>> {
    if interval.len() == 1 {
        let value = interval.get(0);
        let dtype = interval.data_type().clone();
        let scalar = PrimitiveScalar::new(dtype, value);
        return sub_interval_scalar(timestamp, &scalar);
    }
    if timestamp.len() != interval.len() {
        return Err(Error::InvalidArgumentError(
            "Timestamp and interval arrays must have the same length".to_string(),
        ));
    }
    match timestamp.data_type().to_logical_type() {
        DataType::Timestamp(time_unit, Some(timezone_str)) => {
            let time_unit = *time_unit;
            let timezone = temporal_conversions::parse_offset(timezone_str);
            match timezone {
                Ok(timezone) => Ok(binary(
                    timestamp,
                    interval,
                    timestamp.data_type().clone(),
                    |timestamp, interval| {
                        temporal_conversions::sub_interval(
                            timestamp, time_unit, interval, &timezone,
                        )
                    },
                )),
                #[cfg(feature = "chrono-tz")]
                Err(_) => {
                    let timezone = temporal_conversions::parse_offset_tz(timezone_str)?;
                    Ok(binary(
                        timestamp,
                        interval,
                        timestamp.data_type().clone(),
                        |timestamp, interval| {
                            temporal_conversions::sub_interval(
                                timestamp, time_unit, interval, &timezone,
                            )
                        },
                    ))
                }
                #[cfg(not(feature = "chrono-tz"))]
                _ => Err(Error::InvalidArgumentError(format!(
                    "timezone \"{}\" cannot be parsed (feature chrono-tz is not active)",
                    timezone_str
                ))),
            }
        }
        DataType::Timestamp(time_unit, None) => {
            let time_unit = *time_unit;
            Ok(binary(
                timestamp,
                interval,
                timestamp.data_type().clone(),
                |timestamp, interval| {
                    temporal_conversions::sub_naive_interval(timestamp, time_unit, interval)
                },
            ))
        }
        _ => Err(Error::InvalidArgumentError(
            "Adding an interval is only supported for `DataType::Timestamp`".to_string(),
        )),
    }
}

/// Adds an interval to a [`DataType::Timestamp`].
pub fn sub_interval_scalar(
    timestamp: &PrimitiveArray<i64>,
    interval: &PrimitiveScalar<months_days_ns>,
) -> Result<PrimitiveArray<i64>> {
    let interval = if let Some(interval) = *interval.value() {
        interval
    } else {
        return Ok(PrimitiveArray::<i64>::new_null(
            timestamp.data_type().clone(),
            timestamp.len(),
        ));
    };

    match timestamp.data_type().to_logical_type() {
        DataType::Timestamp(time_unit, Some(timezone_str)) => {
            let time_unit = *time_unit;
            let timezone = temporal_conversions::parse_offset(timezone_str);
            match timezone {
                Ok(timezone) => Ok(unary(
                    timestamp,
                    |timestamp| {
                        temporal_conversions::sub_interval(
                            timestamp, time_unit, interval, &timezone,
                        )
                    },
                    timestamp.data_type().clone(),
                )),
                #[cfg(feature = "chrono-tz")]
                Err(_) => {
                    let timezone = temporal_conversions::parse_offset_tz(timezone_str)?;
                    Ok(unary(
                        timestamp,
                        |timestamp| {
                            temporal_conversions::sub_interval(
                                timestamp, time_unit, interval, &timezone,
                            )
                        },
                        timestamp.data_type().clone(),
                    ))
                }
                #[cfg(not(feature = "chrono-tz"))]
                _ => Err(Error::InvalidArgumentError(format!(
                    "timezone \"{}\" cannot be parsed (feature chrono-tz is not active)",
                    timezone_str
                ))),
            }
        }
        DataType::Timestamp(time_unit, None) => {
            let time_unit = *time_unit;
            Ok(unary(
                timestamp,
                |timestamp| {
                    temporal_conversions::sub_naive_interval(timestamp, time_unit, interval)
                },
                timestamp.data_type().clone(),
            ))
        }
        _ => Err(Error::InvalidArgumentError(
            "Adding an interval is only supported for `DataType::Timestamp`".to_string(),
        )),
    }
}

/// Multiplies an interval by a factor.
pub fn mul_interval(
    interval: &PrimitiveArray<months_days_ns>,
    factor: &PrimitiveArray<i32>,
) -> Result<PrimitiveArray<months_days_ns>> {
    if factor.len() == 1 {
        let value = factor.get(0);
        let dtype = factor.data_type().clone();
        let scalar = PrimitiveScalar::new(dtype, value);
        return mul_interval_scalar(interval, &scalar);
    }

    if interval.len() != factor.len() {
        return Err(Error::InvalidArgumentError(
            "Interval and factor arrays must have the same length".to_string(),
        ));
    }

    Ok(binary(
        interval,
        factor,
        interval.data_type().clone(),
        |interval, factor| {
            months_days_ns::new(
                interval.months() * factor,
                interval.days() * factor,
                interval.ns() * factor as i64,
            )
        },
    ))
}

pub fn mul_interval_scalar(
    interval: &PrimitiveArray<months_days_ns>,
    factor: &PrimitiveScalar<i32>,
) -> Result<PrimitiveArray<months_days_ns>> {
    let factor = if let Some(factor) = *factor.value() {
        factor
    } else {
        return Ok(PrimitiveArray::<months_days_ns>::new_null(
            interval.data_type().clone(),
            interval.len(),
        ));
    };

    Ok(unary(
        interval,
        |interval| {
            months_days_ns::new(
                interval.months() * factor,
                interval.days() * factor,
                interval.ns() * factor as i64,
            )
        },
        interval.data_type().clone(),
    ))
}
