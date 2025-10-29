use terminal_size::{Width, terminal_size};

use crate::{database::local::SqlTransaction, errors::CatError};

pub async fn list_pacs() -> Result<(), CatError> {
    let mut tx = SqlTransaction::new().await?;
    let pacs = tx.get_pac_names().await?;
    if pacs.is_empty() {
        println!("No packages installed.");
    } else {
        println!("Installed packages:\n");
        let size = terminal_size();
        if let Some((Width(w), _)) = size {
            print_columns_vertical(&pacs);
        } else {
            for pac in pacs {
                println!("{}", pac);
            }
        }
    }
    Ok(())
}

fn print_columns_vertical(items: &[String]) {
    if items.is_empty() {
        return;
    }

    // Terminal width (default to 80 if unknown)
    let term_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    let n = items.len();

    // Try maximum possible columns and find the one that fits best
    let mut best_cols = 1;
    let mut best_widths = vec![0];

    for cols in (1..=n).rev() {
        let rows = (n + cols - 1) / cols;
        let mut widths = vec![0; cols];

        for col in 0..cols {
            for row in 0..rows {
                if let Some(item) = items.get(row + col * rows) {
                    widths[col] = widths[col].max(item.len());
                }
            }
        }

        let total_width = widths.iter().sum::<usize>() + 4 * (cols - 1) + 4;
        if total_width <= term_width {
            best_cols = cols;
            best_widths = widths;
            break;
        }
    }

    let cols = best_cols;
    let rows = (n + cols - 1) / cols;

    // Output items top-to-bottom, left-to-right
    for row in 0..rows {
        for col in 0..cols {
            if let Some(item) = items.get(row + col * rows) {
                if col == cols - 1 {
                    print!("{item}");
                } else {
                    print!("{:<width$}", item, width = best_widths[col] + 4);
                }
            }
        }
        println!();
    }
}
