import Foundation

extension WorksheetModel {


  internal func evaluateFinancialAndDateFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "PMT":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue,
        nper != 0.0
      else { return .error("#VALUE!") }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let typeVal = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let type = typeVal != 0.0 ? 1.0 : 0.0
      if rate == 0.0 {
        let res = -(pv + fv) / nper
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      let denom = (1.0 + rate * type) * (factor - 1.0)
      guard factor.isFinite, denom != 0.0 else { return .error("#NUM!") }
      let payment = -rate * (pv * factor + fv) / denom
      return payment.isFinite ? .number(payment) : .error("#NUM!")

    case "IPMT", "PPMT":
      guard (4...6).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let per = frame.arg(1, in: self).numericValue,
        let nper = frame.arg(2, in: self).numericValue,
        let pv = frame.arg(3, in: self).numericValue
      else { return .error("#VALUE!") }
      guard nper > 0.0, per >= 1.0, per <= nper else { return .error("#NUM!") }
      let fv = frame.count >= 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let typeVal = frame.count == 6 ? (frame.arg(5, in: self).numericValue ?? 0.0) : 0.0
      let type = typeVal != 0.0 ? 1.0 : 0.0
      let pmt: Double
      if rate == 0.0 {
        pmt = -(pv + fv) / nper
      } else {
        let factor = pow(1.0 + rate, nper)
        let denom = (1.0 + rate * type) * (factor - 1.0)
        guard factor.isFinite, denom != 0.0 else { return .error("#NUM!") }
        pmt = -rate * (pv * factor + fv) / denom
      }
      let ipmt: Double
      if rate == 0.0 || (type != 0.0 && per == 1.0) {
        ipmt = 0.0
      } else {
        let nPeriods = type != 0.0 ? (per - 2.0) : (per - 1.0)
        let factor = pow(1.0 + rate, nPeriods)
        guard factor.isFinite else { return .error("#NUM!") }
        let fvBefore = -(pv * factor + pmt * (1.0 + rate * type) * (factor - 1.0) / rate)
        ipmt = type != 0.0 ? (fvBefore - pmt) * rate : fvBefore * rate
      }
      let res = (name == "IPMT") ? ipmt : (pmt - ipmt)
      return res.isFinite ? .number(res) : .error("#NUM!")

    case "FV":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pmt = frame.arg(2, in: self).numericValue
      else { return .error("#VALUE!") }
      let pv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let typeVal = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let type = typeVal != 0.0 ? 1.0 : 0.0
      if rate == 0.0 {
        let res = -(pv + pmt * nper)
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      let futureVal = -(pv * factor + pmt * (1.0 + rate * type) * (factor - 1.0) / rate)
      return futureVal.isFinite ? .number(futureVal) : .error("#NUM!")

    case "PV":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pmt = frame.arg(2, in: self).numericValue
      else { return .error("#VALUE!") }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let typeVal = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let type = typeVal != 0.0 ? 1.0 : 0.0
      if rate == 0.0 {
        let res = -(fv + pmt * nper)
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      guard factor.isFinite, factor != 0.0 else { return .error("#NUM!") }
      let presentVal = -(fv + pmt * (1.0 + rate * type) * (factor - 1.0) / rate) / factor
      return presentVal.isFinite ? .number(presentVal) : .error("#NUM!")

    case "NPV":
      guard frame.count >= 2, let rate = frame.arg(0, in: self).numericValue, rate != -1.0 else {
        return .error("#VALUE!")
      }
      switch frame.resolve(1..<frame.count, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let cashFlows = vals.compactMap { $0.numericValue }
        var npv = 0.0
        for (idx, cf) in cashFlows.enumerated() {
          npv += cf / pow(1.0 + rate, Double(idx + 1))
        }
        return npv.isFinite ? .number(npv) : .error("#NUM!")
      }

    case "IRR":
      guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
      switch frame.resolve(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        let rawCashFlows = vals.compactMap(\.numericValue)
        guard rawCashFlows.contains(where: { $0 > 0.0 }),
          rawCashFlows.contains(where: { $0 < 0.0 })
        else {
          return .error("#NUM!")
        }
        let maxMag = max(1.0, rawCashFlows.map { abs($0) }.max() ?? 1.0)
        let cashFlows = rawCashFlows.map { $0 / maxMag }

        func evalNPV(_ r: Double) -> Double {
          let base = 1.0 + r
          guard base > 1e-9 else { return .nan }
          var sum = 0.0
          for (t, cf) in cashFlows.enumerated() {
            sum += cf / pow(base, Double(t))
          }
          return sum
        }

        let guess = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 0.1) : 0.1
        var rate = guess <= -1.0 ? 0.1 : guess
        var lastResidual = Double.infinity
        for _ in 0..<100 {
          var npv = 0.0
          var dNpv = 0.0
          let base = 1.0 + rate
          guard base > 1e-9 else {
            rate = 0.05
            continue
          }
          for (t, cf) in cashFlows.enumerated() {
            let tDouble = Double(t)
            npv += cf / pow(base, tDouble)
            if t > 0 {
              dNpv -= tDouble * cf / pow(base, tDouble + 1.0)
            }
          }
          lastResidual = abs(npv)
          if lastResidual < 1e-9 {
            return .number(rate)
          }
          guard abs(dNpv) > 1e-12, dNpv.isFinite else {
            break
          }
          let nextRate = rate - npv / dNpv
          if abs(nextRate - rate) < 1e-9, lastResidual < 1e-5 {
            return .number(nextRate)
          }
          rate = max(-0.9999, nextRate)
        }
        if rate.isFinite, lastResidual < 1e-5 {
          return .number(rate)
        }
        let samplePoints: [Double] = [
          -0.99, -0.95, -0.9, -0.8, -0.7, -0.6, -0.5, -0.4, -0.3, -0.2, -0.1,
          0.0, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0, 7.5, 10.0,
        ]
        if let root = Self.bisectRoot(samplePoints: samplePoints, tol: 1e-4, eval: evalNPV) {
          return .number(root)
        }
        return .error("#NUM!")
      }

    case "NPER":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let pmt = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      if rate == 0.0 {
        guard pmt != 0.0 else { return .error("#DIV/0!") }
        let nper = -(pv + fv) / pmt
        return nper.isFinite ? .number(nper) : .error("#NUM!")
      }
      let pmtAdj = pmt * (1.0 + rate * type)
      let num = pmtAdj - fv * rate
      let den = pv * rate + pmtAdj
      guard den != 0.0, (num / den) > 0.0, (1.0 + rate) > 0.0 else {
        return .error("#NUM!")
      }
      let result = log(num / den) / log(1.0 + rate)
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "RATE":
      guard (3...6).contains(frame.count),
        let nper = frame.arg(0, in: self).numericValue,
        let pmt = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue,
        nper > 0.0
      else {
        return .error("#VALUE!")
      }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let typeVal = frame.count >= 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let type = typeVal != 0.0 ? 1.0 : 0.0
      let guess = frame.count == 6 ? (frame.arg(5, in: self).numericValue ?? 0.1) : 0.1
      let scale = max(1.0, abs(pv), abs(pmt * nper), abs(fv))
      let tol = 1e-7 * scale

      func evalTVM(_ r: Double) -> Double {
        if abs(r) < 1e-12 {
          return pv + pmt * nper + fv
        }
        let factor = pow(1.0 + r, nper)
        return pv * factor + pmt * (1.0 + r * type) * (factor - 1.0) / r + fv
      }

      var r = guess <= -1.0 ? 0.1 : guess
      let eps = 1e-7
      for _ in 0..<100 {
        let y = evalTVM(r)
        if abs(y) <= tol {
          return .number(r)
        }
        let dy = (evalTVM(r + eps) - evalTVM(r - eps)) / (2.0 * eps)
        guard abs(dy) > 1e-12, dy.isFinite else {
          break
        }
        let nextR = r - y / dy
        if abs(nextR - r) <= 1e-10, abs(evalTVM(nextR)) <= tol * 100.0 {
          return .number(nextR)
        }
        r = max(-0.9999, nextR)
      }
      if abs(evalTVM(r)) <= tol * 100.0 && r.isFinite {
        return .number(r)
      }
      let samplePoints: [Double] = [
        -0.99, -0.95, -0.9, -0.8, -0.7, -0.6, -0.5, -0.4, -0.3, -0.2, -0.1,
        0.0, 0.001, 0.005, 0.01, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0, 7.5,
        10.0,
      ]
      if let root = Self.bisectRoot(samplePoints: samplePoints, tol: tol * 100.0, eval: evalTVM) {
        return .number(root)
      }
      return .error("#NUM!")

    case "EFFECT":
      guard frame.count == 2,
        let nominal = frame.arg(0, in: self).numericValue,
        let nperyVal = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let npery = floor(nperyVal)
      guard nominal > 0.0, npery >= 1.0 else {
        return .error("#NUM!")
      }
      let result = pow(1.0 + nominal / npery, npery) - 1.0
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "NOMINAL":
      guard frame.count == 2,
        let effect = frame.arg(0, in: self).numericValue,
        let nperyVal = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let npery = floor(nperyVal)
      guard effect > 0.0, npery >= 1.0 else {
        return .error("#NUM!")
      }
      let result = npery * (pow(1.0 + effect, 1.0 / npery) - 1.0)
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "SLN":
      guard frame.count == 3,
        let cost = frame.arg(0, in: self).numericValue,
        let salvage = frame.arg(1, in: self).numericValue,
        let life = frame.arg(2, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      guard life != 0.0 else {
        return .error("#DIV/0!")
      }
      return .number((cost - salvage) / life)

    case "SYD":
      guard frame.count == 4,
        let cost = frame.arg(0, in: self).numericValue,
        let salvage = frame.arg(1, in: self).numericValue,
        let life = frame.arg(2, in: self).numericValue,
        let per = frame.arg(3, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      guard life > 0.0, per > 0.0, per <= life else {
        return .error("#NUM!")
      }
      let result = ((cost - salvage) * (life - per + 1.0) * 2.0) / (life * (life + 1.0))
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "TODAY":
      guard frame.count == 0 else { return .error("#VALUE!") }
      let now = Date()
      let comps = Self.utcCalendar.dateComponents([.year, .month, .day], from: now)
      if let utcToday = Self.makeUTCDate(
        year: comps.year ?? 2024, month: comps.month ?? 1, day: comps.day ?? 1),
        let serial = Self.excelSerialNumber(from: utcToday)
      {
        return .number(serial)
      }
      return .number(45292.0)

    case "NOW":
      guard frame.count == 0 else { return .error("#VALUE!") }
      let now = Date()
      let comps = Self.utcCalendar.dateComponents(
        [.year, .month, .day, .hour, .minute, .second], from: now)
      if let utcToday = Self.makeUTCDate(
        year: comps.year ?? 2024, month: comps.month ?? 1, day: comps.day ?? 1),
        let serial = Self.excelSerialNumber(from: utcToday)
      {
        let hour: Int = comps.hour ?? 0
        let minute: Int = comps.minute ?? 0
        let second: Int = comps.second ?? 0
        let totalSeconds: Int = hour * 3600 + minute * 60 + second
        let frac: Double = Double(totalSeconds) / 86400.0
        return .number(serial + frac)
      }
      return .number(45292.0)

    case "DATEVALUE":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let argVal = frame.arg(0, in: self)
      if case .error = argVal { return argVal }
      guard let date = Self.parseDateValue(argVal),
        let serial = Self.excelSerialNumber(from: date)
      else { return .error("#VALUE!") }
      return .number(serial)

    case "TIME":
      guard frame.count == 3,
        let hVal = frame.arg(0, in: self).numericValue,
        let mVal = frame.arg(1, in: self).numericValue,
        let sVal = frame.arg(2, in: self).numericValue,
        let h = Self.safeInt(trunc(hVal)),
        let m = Self.safeInt(trunc(mVal)),
        let s = Self.safeInt(trunc(sVal))
      else { return .error("#VALUE!") }
      guard (0...32767).contains(h), (0...32767).contains(m), (0...32767).contains(s) else {
        return .error("#NUM!")
      }
      let totalSecs = h * 3600 + m * 60 + s
      return .number(Double(totalSecs % 86400) / 86400.0)

    case "HOUR", "MINUTE", "SECOND":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue, num >= 0.0 else {
        return .error("#VALUE!")
      }
      let frac = num - floor(num)
      let totalSecs = Int((frac * 86400.0).rounded()) % 86400
      if name == "HOUR" { return .number(Double(totalSecs / 3600)) }
      if name == "MINUTE" { return .number(Double((totalSecs % 3600) / 60)) }
      return .number(Double(totalSecs % 60))

    case "DATE":
      guard frame.count == 3,
        let yNum = frame.arg(0, in: self).numericValue,
        let mNum = frame.arg(1, in: self).numericValue,
        let dNum = frame.arg(2, in: self).numericValue,
        let y = Self.safeInt(yNum), let m = Self.safeInt(mNum), let d = Self.safeInt(dNum)
      else { return .error("#VALUE!") }
      guard abs(m) <= 120_000, abs(d) <= 4_000_000 else { return .error("#NUM!") }
      let resolvedY = (0...1899).contains(y) ? (1900 + y) : y
      if resolvedY == 1900 && m == 2 && d == 29 {
        return .number(60.0)
      }
      guard let date = Self.makeUTCDate(year: y, month: m, day: d, allowRollover: true),
        let serial = Self.excelSerialNumber(from: date)
      else { return .error("#VALUE!") }
      return .number(serial)

    case "YEAR":
      if frame.count == 1, let raw = frame.arg(0, in: self).numericValue,
        Self.safeInt(floor(raw)) == 60
      {
        return .number(1900.0)
      }
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.year, from: date)))

    case "MONTH":
      if frame.count == 1, let raw = frame.arg(0, in: self).numericValue,
        Self.safeInt(floor(raw)) == 60
      {
        return .number(2.0)
      }
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.month, from: date)))

    case "DAY":
      if frame.count == 1, let raw = frame.arg(0, in: self).numericValue,
        Self.safeInt(floor(raw)) == 60
      {
        return .number(29.0)
      }
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.day, from: date)))

    case "DAYS":
      guard frame.count == 2,
        let endDate = Self.parseDateValue(frame.arg(0, in: self)),
        let startDate = Self.parseDateValue(frame.arg(1, in: self))
      else { return .error("#VALUE!") }
      return .number(
        Double(Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0))

    case "WEEKDAY", "WEEKNUM":
      guard (1...2).contains(frame.count),
        let date = Self.parseDateValue(frame.arg(0, in: self))
      else {
        return .error("#VALUE!")
      }
      let returnType: Int
      if frame.count == 2 {
        guard let rtNum = frame.arg(1, in: self).numericValue,
          let rtInt = Self.safeInt(rtNum)
        else {
          return .error("#VALUE!")
        }
        returnType = rtInt
      } else {
        returnType = 1
      }
      if name == "WEEKDAY" {
        let calWeekday = Self.utcCalendar.component(.weekday, from: date)
        switch returnType {
        case 1:
          return .number(Double(calWeekday))
        case 2:
          return .number(Double(calWeekday == 1 ? 7 : calWeekday - 1))
        case 3:
          return .number(Double(calWeekday == 1 ? 6 : calWeekday - 2))
        default:
          return .error("#NUM!")
        }
      }
      var cal = Self.utcCalendar
      cal.firstWeekday = (returnType == 2 || returnType == 21) ? 2 : 1
      cal.minimumDaysInFirstWeek = 1
      return .number(Double(cal.component(.weekOfYear, from: date)))

    case "EDATE":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let mNum = frame.arg(1, in: self).numericValue,
        let months = Self.safeInt(trunc(mNum)),
        abs(months) <= 120_000,
        let targetDate = Self.utcCalendar.date(byAdding: .month, value: months, to: startDate),
        let serial = Self.excelSerialNumber(from: targetDate)
      else {
        return .error("#VALUE!")
      }
      return .number(serial)

    case "EOMONTH":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let mNum = frame.arg(1, in: self).numericValue,
        let months = Self.safeInt(trunc(mNum)),
        abs(months) <= 120_000
      else {
        return .error("#VALUE!")
      }
      let startComps = Self.utcCalendar.dateComponents([.year, .month], from: startDate)
      guard
        let firstOfStartMonth = Self.makeUTCDate(
          year: startComps.year ?? 1970, month: startComps.month ?? 1, day: 1),
        let firstOfNextAfterTarget = Self.utcCalendar.date(
          byAdding: .month, value: months + 1, to: firstOfStartMonth),
        let lastDayOfTargetMonth = Self.utcCalendar.date(
          byAdding: .day, value: -1, to: firstOfNextAfterTarget),
        let serial = Self.excelSerialNumber(from: lastDayOfTargetMonth)
      else {
        return .error("#VALUE!")
      }
      return .number(serial)

    case "WORKDAY":
      guard (2...3).contains(frame.count),
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let dNum = frame.arg(1, in: self).numericValue,
        let days = Self.safeInt(trunc(dNum))
      else {
        return .error("#VALUE!")
      }
      guard abs(days) <= 100_000 else {
        return .error("#NUM!")
      }
      let holidayDates: Set<Date>
      switch resolveHolidayDates(from: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let dates):
        holidayDates = dates
      }
      var current = startDate
      var remaining = abs(days)
      let step = days >= 0 ? 1 : -1
      while remaining > 0 {
        guard let next = Self.utcCalendar.date(byAdding: .day, value: step, to: current) else {
          return .error("#VALUE!")
        }
        current = next
        let wd = Self.utcCalendar.component(.weekday, from: current)
        if wd != 1 && wd != 7 && !holidayDates.contains(current) {
          remaining -= 1
        }
      }
      guard let serial = Self.excelSerialNumber(from: current) else {
        return .error("#VALUE!")
      }
      return .number(serial)

    case "NETWORKDAYS":
      guard (2...3).contains(frame.count),
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let endDate = Self.parseDateValue(frame.arg(1, in: self))
      else {
        return .error("#VALUE!")
      }
      let holidayDates: Set<Date>
      switch resolveHolidayDates(from: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let dates):
        holidayDates = dates
      }
      let isReverse = startDate > endDate
      let fromDate = isReverse ? endDate : startDate
      let toDate = isReverse ? startDate : endDate
      let daySpan = abs(
        Self.utcCalendar.dateComponents([.day], from: fromDate, to: toDate).day ?? 0)
      guard daySpan <= 100_000 else {
        return .error("#NUM!")
      }
      var current = fromDate
      var count = 0
      while current <= toDate {
        let wd = Self.utcCalendar.component(.weekday, from: current)
        if wd != 1 && wd != 7 && !holidayDates.contains(current) {
          count += 1
        }
        guard let next = Self.utcCalendar.date(byAdding: .day, value: 1, to: current) else {
          break
        }
        current = next
      }
      return .number(Double(isReverse ? -count : count))

    case "DATEDIF":
      guard frame.count == 3,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let endDate = Self.parseDateValue(frame.arg(1, in: self))
      else {
        return .error("#VALUE!")
      }
      guard startDate <= endDate else {
        return .error("#NUM!")
      }
      let unit = frame.arg(2, in: self).description.trimmingCharacters(in: .whitespacesAndNewlines)
        .uppercased()
      switch unit {
      case "D":
        let days = Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0
        return .number(Double(days))
      case "M":
        let months =
          Self.utcCalendar.dateComponents([.month], from: startDate, to: endDate).month ?? 0
        return .number(Double(months))
      case "Y":
        let years = Self.utcCalendar.dateComponents([.year], from: startDate, to: endDate).year ?? 0
        return .number(Double(years))
      case "YM":
        let months =
          Self.utcCalendar.dateComponents([.year, .month], from: startDate, to: endDate).month ?? 0
        return .number(Double(months))
      case "MD":
        let days =
          Self.utcCalendar.dateComponents([.year, .month, .day], from: startDate, to: endDate).day
          ?? 0
        return .number(Double(days))
      default:
        return .error("#VALUE!")
      }

    default:
      return nil
    }
  }


  internal func evaluateInfoFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
    switch name {
    case "ISBLANK":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      return .boolean(val == .empty)
    case "ISERROR":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .error = val { return .boolean(true) }
      return .boolean(false)
    case "ISERR":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .error(let code) = val { return .boolean(code != "#N/A") }
      return .boolean(false)
    case "ISNA":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      return .boolean(val == .error("#N/A"))
    case "ISNUMBER":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .number = val { return .boolean(true) }
      return .boolean(false)
    case "ISTEXT":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .text = val { return .boolean(true) }
      return .boolean(false)
    case "ISNONTEXT":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .text = val { return .boolean(false) }
      return .boolean(true)
    case "ISLOGICAL":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .boolean = val { return .boolean(true) }
      return .boolean(false)
    case "ISEVEN", "ISODD":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .error = val { return val }
      if case .boolean = val { return .error("#VALUE!") }
      guard let num = val.numericValue, let i = Self.safeInt(trunc(num)) else {
        return .error("#VALUE!")
      }
      return .boolean(name == "ISEVEN" ? (i % 2 == 0) : (i % 2 != 0))
    case "TYPE":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      switch val {
      case .number, .empty: return .number(1.0)
      case .text: return .number(2.0)
      case .boolean: return .number(4.0)
      case .error: return .number(16.0)
      }
    case "NA":
      guard frame.count == 0 else { return .error("#VALUE!") }
      return .error("#N/A")
    case "N":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      switch val {
      case .number(let n): return .number(n)
      case .boolean(let b): return .number(b ? 1.0 : 0.0)
      case .error(let e): return .error(e)
      case .text, .empty: return .number(0.0)
      }
    default:
      return nil
    }
  }

  internal func rangeBounds(_ range: CellRange, allowWholeColumnOrRow: Bool = false) -> (
    minRow: Int, maxRow: Int, minCol: Int, maxCol: Int, rowCount: Int, colCount: Int
  )? {
    let minRow = min(range.start.row, range.end.row)
    let maxRow = max(range.start.row, range.end.row)
    let minCol = min(range.start.column, range.end.column)
    let maxCol = max(range.start.column, range.end.column)
    let (rowSpan, rOverflow) = (maxRow - minRow).addingReportingOverflow(1)
    let (colSpan, cOverflow) = (maxCol - minCol).addingReportingOverflow(1)
    let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !rOverflow, !cOverflow, !mulOverflow,
      totalCells > 0,
      totalCells <= CellRange.maxExpandedCellCount
        || (allowWholeColumnOrRow && range.isWholeColumnOrRow)
    else {
      return nil
    }
    return (minRow, maxRow, minCol, maxCol, rowSpan, colSpan)
  }

  internal func evaluateSingleCellInRange(
    onSheet sheetName: String?,
    row: Int,
    column: Int,
    context: inout EvaluationContext,
    depth: Int,
    coerceEmptyToZero: Bool = true
  ) -> EvaluatedCellValue {
    let coord = PackedCellCoordinate(row: row, column: column)
    let val =
      sheetName.map {
        evaluateOnSheet(named: $0, at: coord, context: &context, depth: depth)
      } ?? evaluateInternal(at: coord, context: &context, depth: depth)
    if coerceEmptyToZero, case .empty = val {
      return .number(0.0)
    }
    return val
  }

  internal static let utcCalendar: Calendar = {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = TimeZone(secondsFromGMT: 0)!
    return cal
  }()

  internal static func makeUTCDate(
    year: Int, month: Int, day: Int, allowRollover: Bool = false
  ) -> Date? {
    let resolvedYear = (allowRollover && (0...1899).contains(year)) ? (1900 + year) : year
    guard (1...9999).contains(resolvedYear) else { return nil }
    if allowRollover {
      var baseComps = DateComponents()
      baseComps.year = resolvedYear
      baseComps.month = 1
      baseComps.day = 1
      baseComps.hour = 0
      baseComps.minute = 0
      baseComps.second = 0
      guard let jan1 = utcCalendar.date(from: baseComps),
        let monthShifted = utcCalendar.date(byAdding: .month, value: month - 1, to: jan1),
        let dayShifted = utcCalendar.date(byAdding: .day, value: day - 1, to: monthShifted)
      else {
        return nil
      }
      return dayShifted
    }
    guard (1...12).contains(month), (1...31).contains(day) else { return nil }
    var comps = DateComponents()
    comps.year = resolvedYear
    comps.month = month
    comps.day = day
    comps.hour = 0
    comps.minute = 0
    comps.second = 0
    guard let constructed = utcCalendar.date(from: comps),
      utcCalendar.component(.month, from: constructed) == month,
      utcCalendar.component(.day, from: constructed) == day
    else {
      return nil
    }
    return constructed
  }

  internal static func formatUTCDate(_ date: Date) -> String {
    let comps = utcCalendar.dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", comps.year ?? 1970, comps.month ?? 1, comps.day ?? 1)
  }

  internal static func excelSerialNumber(from date: Date) -> Double? {
    var epoch1899Dec31 = DateComponents()
    epoch1899Dec31.year = 1899
    epoch1899Dec31.month = 12
    epoch1899Dec31.day = 31
    guard let base = utcCalendar.date(from: epoch1899Dec31),
      let rawDays = utcCalendar.dateComponents([.day], from: base, to: date).day
    else { return nil }
    guard rawDays >= 0 else { return nil }
    let serial = rawDays >= 60 ? (rawDays + 1) : rawDays
    return Double(serial)
  }

  internal static func parseDateValue(_ value: EvaluatedCellValue) -> Date? {
    if case .number(let n) = value, n.isFinite {
      guard let intVal = safeInt(floor(n)) else { return nil }
      if intVal >= 19_000_101 && intVal <= 99_991_231 {
        let y = intVal / 10_000
        let m = (intVal / 100) % 100
        let d = intVal % 100
        if let ymd = makeUTCDate(year: y, month: m, day: d, allowRollover: false) {
          return ymd
        }
      }
      if intVal >= 1 && intVal <= 2_958_465 {
        var epochComps = DateComponents()
        epochComps.year = 1899
        epochComps.month = 12
        epochComps.day = intVal < 60 ? 31 : 30
        if let base = utcCalendar.date(from: epochComps) {
          return utcCalendar.date(byAdding: .day, value: intVal, to: base)
        }
      }
    }
    let raw = value.description.trimmingCharacters(in: .whitespacesAndNewlines)
    let separators = CharacterSet(charactersIn: "-/.")
    let parts = raw.components(separatedBy: separators)
    if parts.count == 3,
      let y = Int(parts[0]),
      let m = Int(parts[1]),
      let d = Int(parts[2])
    {
      return makeUTCDate(year: y, month: m, day: d, allowRollover: false)
    }
    return nil
  }

  private static func bisectRoot(
    samplePoints: [Double],
    tol: Double,
    eval: (Double) -> Double
  ) -> Double? {
    guard !samplePoints.isEmpty else { return nil }
    var prevR = samplePoints[0]
    var prevY = eval(prevR)
    for idx in 1..<samplePoints.count {
      let curR = samplePoints[idx]
      let curY = eval(curR)
      if prevY.isFinite, curY.isFinite, prevY * curY <= 0.0 {
        var lo = prevR
        var hi = curR
        var fLo = prevY
        for _ in 0..<80 {
          let mid = 0.5 * (lo + hi)
          let fMid = eval(mid)
          if !fMid.isFinite { break }
          if abs(fMid) <= tol || (hi - lo) < 1e-11 {
            return mid
          }
          if fLo * fMid <= 0.0 {
            hi = mid
          } else {
            lo = mid
            fLo = fMid
          }
        }
        let mid = 0.5 * (lo + hi)
        if abs(eval(mid)) <= tol * 10.0 {
          return mid
        }
      }
      prevR = curR
      prevY = curY
    }
    return nil
  }

  private enum HolidayResolutionResult {
    case success(Set<Date>)
    case failure(String)
  }

  private func resolveHolidayDates(from frame: inout EvalFrame) -> HolidayResolutionResult {
    var holidayDates = Set<Date>()
    if frame.count == 3, frame.args[2] != .empty, frame.args[2] != .missing {
      switch frame.resolve(2..<3, in: self) {
      case .failure(let err):
        return .failure(err)
      case .success(let hVals):
        for hVal in hVals where hVal != .empty {
          guard let hDate = Self.parseDateValue(hVal) else {
            return .failure("#VALUE!")
          }
          holidayDates.insert(hDate)
        }
      }
    }
    return .success(holidayDates)
  }
}
