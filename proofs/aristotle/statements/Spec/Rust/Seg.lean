module

public import SepLogic.Public

@[expose] public section

namespace Mir.Seg

open Rust2Lean.Public

-- Use these names with `Vocab` to state reusable properties.
-- Named handles make no correctness claim about their functions.
def Fn.«percolator::v16::checked_i128_mul» : RustFn := { name := "percolator::v16::checked_i128_mul", id := (77448616941724955046835306141042812992 : Int) }
def Fn.«core::num::<impl i128>::overflowing_add» : RustFn := { name := "core::num::<impl i128>::overflowing_add", id := (165892526789636429945611399034916472352 : Int) }
def Fn.«core::num::<impl i128>::is_negative» : RustFn := { name := "core::num::<impl i128>::is_negative", id := (133306193405843168131914071349404344656 : Int) }
def Fn.«core::str::<impl str>::as_bytes» : RustFn := { name := "core::str::<impl str>::as_bytes", id := (135862955628691765269839677187441824200 : Int) }
def Fn.«core::num::<impl i32>::unsigned_abs» : RustFn := { name := "core::num::<impl i32>::unsigned_abs", id := (102222406169795574468281161822549124328 : Int) }
def Fn.«<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» : RustFn := { name := "<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual", id := (28901881018909679730878975854050444088 : Int) }
def Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}}>» : RustFn := { name := "core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}}>", id := (168863261787305502730414483954044453700 : Int) }
def Fn.«core::option::Option::<u128>::and_then::<u128, {closure@percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}}>» : RustFn := { name := "core::option::Option::<u128>::and_then::<u128, {closure@percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}}>", id := (160080666511626399382018579507809443124 : Int) }
def Fn.«core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}}> - shim(None)» : RustFn := { name := "core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}}> - shim(None)", id := (103026182211934217101233315773651076212 : Int) }
def Fn.«<core::result::Result<(i128, i128, i128, i128), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» : RustFn := { name := "<core::result::Result<(i128, i128, i128, i128), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual", id := (118565671799930086751262390542951717364 : Int) }
def Fn.«core::slice::<impl [u8]>::len» : RustFn := { name := "core::slice::<impl [u8]>::len", id := (160574279778018090677650161301335860124 : Int) }
def Fn.«core::num::<impl u128>::checked_mul» : RustFn := { name := "core::num::<impl u128>::checked_mul", id := (23429884298195247566452777566007312690 : Int) }
def Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas» : RustFn := { name := "percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas", id := (114071401490549818881987734622125095794 : Int) }
def Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}» : RustFn := { name := "percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#0}", id := (104150120280360642651205771448531553034 : Int) }
def Fn.«<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» : RustFn := { name := "<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::Try>::branch", id := (53784003459645926085737895338314700314 : Int) }
def Fn.«core::num::<impl i128>::unsigned_abs» : RustFn := { name := "core::num::<impl i128>::unsigned_abs", id := (129004402165060441327703524308762021722 : Int) }
def Fn.«core::num::<impl i32>::is_negative» : RustFn := { name := "core::num::<impl i32>::is_negative", id := (104702614157434132556112359332063469754 : Int) }
def Fn.«<core::result::Result<(i128, i128, i128, i128), percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» : RustFn := { name := "<core::result::Result<(i128, i128, i128, i128), percolator::v16::V16Error> as core::ops::try_trait::Try>::branch", id := (59416681408712228147698615973368731782 : Int) }
def Fn.«percolator::wide_math::floor_div_signed_conservative_i128» : RustFn := { name := "percolator::wide_math::floor_div_signed_conservative_i128", id := (99847200133256178200622229458887739302 : Int) }
def Fn.«percolator::v16::add_non_min_i128» : RustFn := { name := "percolator::v16::add_non_min_i128", id := (152044199538131257994246797958973107854 : Int) }
def Fn.«<roots>» : RustFn := { name := "<roots>", id := (111346825598767891485696918120015726990 : Int) }
def Fn.«core::option::Option::<i128>::and_then::<i128, {closure@percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}}>» : RustFn := { name := "core::option::Option::<i128>::and_then::<i128, {closure@percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}}>", id := (114523594999564962669069724581016707374 : Int) }
def Fn.«core::num::<impl i128>::checked_mul» : RustFn := { name := "core::num::<impl i128>::checked_mul", id := (163755173204581486606307582953494673070 : Int) }
def Fn.«core::num::<impl i128>::checked_neg» : RustFn := { name := "core::num::<impl i128>::checked_neg", id := (163755239181833820359934713977237252734 : Int) }
def Fn.«core::num::<impl i32>::wrapping_sub» : RustFn := { name := "core::num::<impl i32>::wrapping_sub", id := (105153617574338499221308058000684927361 : Int) }
def Fn.«<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» : RustFn := { name := "<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch", id := (146913755060127999129233463737715911105 : Int) }
def Fn.«core::intrinsics::unlikely» : RustFn := { name := "core::intrinsics::unlikely", id := (102015680453086198088526854363201851617 : Int) }
def Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}» : RustFn := { name := "percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}", id := (122622804650470792046411759610046097193 : Int) }
def Fn.«core::num::<impl u128>::overflowing_mul» : RustFn := { name := "core::num::<impl u128>::overflowing_mul", id := (63041717955937800355273004794978692377 : Int) }
def Fn.«core::num::<impl i128>::wrapping_abs» : RustFn := { name := "core::num::<impl i128>::wrapping_abs", id := (64057307946374913316116635869897712473 : Int) }
def Fn.«core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}}> - shim(None)» : RustFn := { name := "core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_accrual_price_move_within_cap::{closure#0}}> - shim(None)", id := (149413488371172872062168655576096598329 : Int) }
def Fn.«core::num::<impl i128>::overflowing_neg» : RustFn := { name := "core::num::<impl i128>::overflowing_neg", id := (165892407915825187547327811709018414085 : Int) }
def Fn.«core::num::<impl i128>::overflowing_mul» : RustFn := { name := "core::num::<impl i128>::overflowing_mul", id := (165892153766735122258757374918732772485 : Int) }
def Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}}>» : RustFn := { name := "core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}}>", id := (168891419983199518363358452051451218773 : Int) }
def Fn.«percolator::v16::V16Core::kernel_accrual_funding_num» : RustFn := { name := "percolator::v16::V16Core::kernel_accrual_funding_num", id := (83410560208369556141525907329318871669 : Int) }
def Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» : RustFn := { name := "<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch", id := (60823971523100356603519823656000796109 : Int) }
def Fn.«percolator::v16::V16Core::kernel_accrual_index_update» : RustFn := { name := "percolator::v16::V16Core::kernel_accrual_index_update", id := (87993583730089732735516780245397393581 : Int) }
def Fn.«core::num::<impl i32>::wrapping_neg» : RustFn := { name := "core::num::<impl i32>::wrapping_neg", id := (105156316895166520757285193892581105757 : Int) }
def Fn.«core::str::<impl str>::as_ptr» : RustFn := { name := "core::str::<impl str>::as_ptr", id := (51146926204421869741299086106099015037 : Int) }
def Fn.«core::num::<impl i128>::wrapping_sub» : RustFn := { name := "core::num::<impl i128>::wrapping_sub", id := (64058619286045522995337449244718949955 : Int) }
def Fn.«core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}}> - shim(None)» : RustFn := { name := "core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}}> - shim(None)", id := (106106335175262986560327093965207053251 : Int) }
def Fn.«core::num::<impl u64>::abs_diff» : RustFn := { name := "core::num::<impl u64>::abs_diff", id := (101771960821285661041740544061129657043 : Int) }
def Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» : RustFn := { name := "<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual", id := (98982186687151609694649748592318073811 : Int) }
def Fn.«core::mem::size_of::<u64>» : RustFn := { name := "core::mem::size_of::<u64>", id := (117589614837839955869994065409892765235 : Int) }
def Fn.«core::ptr::drop_in_place::<percolator::v16::V16Error> - shim(None)» : RustFn := { name := "core::ptr::drop_in_place::<percolator::v16::V16Error> - shim(None)", id := (139676731930426496357896855466982275507 : Int) }
def Fn.«core::num::<impl i128>::checked_add» : RustFn := { name := "core::num::<impl i128>::checked_add", id := (163755502905771119501278886974804088307 : Int) }
def Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap» : RustFn := { name := "percolator::v16::V16Core::kernel_accrual_price_move_within_cap", id := (62686162920580650371946376357125728171 : Int) }
def Fn.«core::convert::num::<impl core::convert::TryFrom<u128> for i128>::try_from» : RustFn := { name := "core::convert::num::<impl core::convert::TryFrom<u128> for i128>::try_from", id := (80840454569529349484120294563218781979 : Int) }
def Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>» : RustFn := { name := "core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>", id := (52111552698604515156144493543826329691 : Int) }
def Fn.«percolator::v16::validate_non_min_i128» : RustFn := { name := "percolator::v16::validate_non_min_i128", id := (17211552754284186545669066316373679419 : Int) }
def Fn.«core::str::<impl str>::len» : RustFn := { name := "core::str::<impl str>::len", id := (158487094977812692205751313987658244731 : Int) }
def Fn.«core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}}> - shim(None)» : RustFn := { name := "core::ptr::drop_in_place::<{closure@percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}}> - shim(None)", id := (36140811913001056478966950242006492743 : Int) }
def Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» : RustFn := { name := "core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>", id := (11027180236640967284650782897539564743 : Int) }
def Fn.«percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}» : RustFn := { name := "percolator::v16::V16Core::kernel_accrual_funding_num::{closure#0}", id := (5145085180303299667982625270270401767 : Int) }
def Fn.«core::fmt::Arguments::<'_>::from_str» : RustFn := { name := "core::fmt::Arguments::<'_>::from_str", id := (72213781763269989042978836912174212503 : Int) }
def Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}» : RustFn := { name := "percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas::{closure#1}", id := (104150120041438215069127378393000433847 : Int) }
def Fn.«core::num::<impl i32>::wrapping_abs» : RustFn := { name := "core::num::<impl i32>::wrapping_abs", id := (105154936353409774986021633381796990391 : Int) }
def Fn.«core::ptr::metadata::metadata::<[u8]>» : RustFn := { name := "core::ptr::metadata::metadata::<[u8]>", id := (8586619058249235659138233691742537583 : Int) }
def Fn.«<percolator::v16::V16Error as core::convert::From<percolator::v16::V16Error>>::from» : RustFn := { name := "<percolator::v16::V16Error as core::convert::From<percolator::v16::V16Error>>::from", id := (53049272397075653128787038259679659391 : Int) }
def Fn.«core::num::<impl i128>::wrapping_neg» : RustFn := { name := "core::num::<impl i128>::wrapping_neg", id := (64056111219525930814900503618186701055 : Int) }
def Global.«"floor_div_signed_conservative_i1"… [55 bytes] 2550b6a279c23806» : RustGlobal := { name := "\"floor_div_signed_conservative_i1\"… [55 bytes] 2550b6a279c23806", id := (166068053056487779178114706888215746489 : Int) }
def Global.«"floor_div_signed_conservative_i1"… [52 bytes] f6ba71dbb1700623» : RustGlobal := { name := "\"floor_div_signed_conservative_i1\"… [52 bytes] f6ba71dbb1700623", id := (128632886603142570517621101797316737563 : Int) }

end Mir.Seg
